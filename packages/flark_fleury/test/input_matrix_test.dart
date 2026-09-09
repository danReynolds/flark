/// Random real input through the mounted host: typed text, keys and clicks
/// against a live FleuryTester, checking after every event that the caret is
/// still a legal offset, that the host reports it to Fleury, and that it lands
/// inside the viewport. A failure prints the seed and the event log.
library;

import 'dart:convert';
import 'dart:math';

import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:test/test.dart';

final failures = <String, List<String>>{};
void fail_(String rule, String d) =>
    (failures[rule] ??= []).length < 5 ? failures[rule]!.add(d) : null;

void main() {
  final backend = createParseBackend();
  final keys = <(KeyCode, Set<KeyModifier>)>[
    (KeyCode.arrowLeft, {}),
    (KeyCode.arrowRight, {}),
    (KeyCode.arrowUp, {}),
    (KeyCode.arrowDown, {}),
    (KeyCode.arrowLeft, {KeyModifier.shift}),
    (KeyCode.arrowDown, {KeyModifier.shift}),
    (KeyCode.home, {}),
    (KeyCode.end, {}),
    (KeyCode.backspace, {}),
    (KeyCode.delete, {}),
    (KeyCode.enter, {}),
    (KeyCode.tab, {}),
    (KeyCode.tab, {KeyModifier.shift}),
    (KeyCode.a, {KeyModifier.superKey}),
    (KeyCode.b, {KeyModifier.superKey}),
    (KeyCode.i, {KeyModifier.superKey}),
    (KeyCode.z, {KeyModifier.superKey}),
    (KeyCode.z, {KeyModifier.superKey, KeyModifier.shift}),
    (KeyCode.escape, {}),
  ];
  const text = ['a', ' ', '*', '#', '-', '>', '`', '|', '\n', 'é', '😀'];
  final sources = [
    '# Title\n\n- one\n- [x] two\n\n```dart\nx\n```\n\n| a | b |\n| - | - |\n| c |\n',
    'plain **bold** text with a [link](http://x.y) and `code`\n',
    '',
    '> quote\n> more\n\n    indented code\n',
  ];

  test('random input keeps the caret legal, reported and on screen', () {
    final master = Random(5);
    for (var trial = 0; trial < 40; trial++) {
      final seed = master.nextInt(1 << 30);
      final r = Random(seed);
      final source = sources[r.nextInt(sources.length)];
      final editor = FlarkEditor(backend, text: source, caret: 0);
      final controller = FlarkFleuryController(editor);
      final focus = FocusNode();
      final tester = FleuryTester(viewportSize: CellSize(r.nextInt(30) + 8, 10));
      final log = <String>[];
      try {
        tester.pumpWidget(Theme(
          data: const ThemeData(),
          child: FlarkEditorView(
              controller: controller, autofocus: true, focusNode: focus),
        ));
        tester.render();
        for (var step = 0; step < 60; step++) {
          if (r.nextInt(3) == 0) {
            final t = text[r.nextInt(text.length)];
            log.add('type ${jsonEncode(t)}');
            tester.type(t);
          } else if (r.nextInt(12) == 0) {
            final cols = tester.viewportSize.cols, rows = tester.viewportSize.rows;
            final c = r.nextInt(cols), y = r.nextInt(rows);
            log.add('click $c,$y');
            tester.sendMouse(MouseEvent(
                button: MouseButton.left,
                kind: MouseEventKind.down,
                col: c,
                row: y));
            tester.sendMouse(MouseEvent(
                button: MouseButton.left,
                kind: MouseEventKind.up,
                col: c,
                row: y));
          } else {
            final (code, mods) = keys[r.nextInt(keys.length)];
            log.add("key $code $mods");
            tester.sendKey(KeyEvent(code, modifiers: mods));
          }
          tester.render();
          // The caret must stay legal and, when focused and collapsed, be
          // reported to the host inside the viewport.
          if (!editor.sourceMode) {
            final whole = !editor.selection.isCollapsed &&
                editor.selection.start == 0 &&
                editor.selection.end == editor.source.length;
            if (!whole && !editor.document.isLegal(editor.selection.extent)) {
              fail_('illegal-caret', 'seed $seed step $step '
                  '${jsonEncode(editor.source)} ${editor.selection}');
            }
          }
          final rect = focus.caretRect;
          if (rect != null &&
              (rect.left < 0 ||
                  rect.top < 0 ||
                  rect.left >= tester.viewportSize.cols ||
                  rect.top >= tester.viewportSize.rows)) {
            fail_('caret-outside-viewport', 'seed $seed step $step $rect '
                'viewport ${tester.viewportSize}');
          }
          if (focus.hasFocus &&
              editor.selection.isCollapsed &&
              rect == null &&
              !editor.sourceMode) {
            fail_('caret-not-reported', 'seed $seed step $step '
                '${jsonEncode(editor.source)} ${editor.selection}');
          }
        }
      } catch (error) {
        fail_('threw', 'seed $seed: $error\n  ${log.join('\n  ')}');
      } finally {
        tester.dispose();
        controller.dispose();
        focus.dispose();
      }
    }
    for (final e in failures.entries) {
      // ignore: avoid_print
      print('### ${e.key}');
      for (final d in e.value) {
        // ignore: avoid_print
        print('    $d');
      }
    }
    expect(failures.keys, isEmpty);
  }, timeout: const Timeout(Duration(minutes: 5)));
}
