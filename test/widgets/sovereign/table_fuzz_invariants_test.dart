import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('Sovereign table fuzz invariants', () {
    test('Mixed table-like edits preserve controller invariants', () {
      const seeds = <int>[5, 13, 23, 37, 61];
      const fixtures = <String>[
        '| a | b |\n| --- | --- |\n| c | d |',
        '| a | b |\n| c | d |',
        '| a \\| b | c |\n| ------ | --- |\n| x      | y   |',
        '| a | b |\n| --- |\n| c | d | e |',
        '> | a | b |\n> | --- | --- |\n> | c | d |',
        '```\n| a | b |\n```',
        '| aa | bb |\n| --- | --- |\n| cc | dd |\n\n| x | y |\n| --- | --- |\n| z | q |',
      ];

      for (final seed in seeds) {
        final rand = Random(seed);
        for (final fixture in fixtures) {
          final controller = SovereignController(
            text: fixture,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          addTearDown(controller.dispose);
          controller.selection = TextSelection.collapsed(
            offset: min(controller.text.length, max(0, fixture.length ~/ 2)),
          );

          for (var step = 0; step < 120; step++) {
            final action = rand.nextInt(10);
            switch (action) {
              case 0:
                _insertRandomChar(controller, rand);
                break;
              case 1:
                controller.handleEnter();
                break;
              case 2:
                _backspace(controller);
                break;
              case 3:
                controller.handleTabKey(reverse: false);
                break;
              case 4:
                controller.handleTabKey(reverse: true);
                break;
              case 5:
                controller.handleArrowDownKey();
                break;
              case 6:
                controller.handleArrowUpKey();
                break;
              case 7:
                _moveCaretRandom(controller, rand);
                break;
              case 8:
                _replaceSelectionWithRandomChar(controller, rand);
                break;
              case 9:
                _toggleRandomComposingRange(controller, rand);
                break;
            }

            _expectControllerInvariants(
              controller,
              seed: seed,
              step: step,
              fixture: fixture,
            );
          }
        }
      }
    });

    test(
      'Malformed table-like rows do not throw on repeated tab traversal',
      () {
        const docs = <String>[
          '| a | b |\n| --- |\n| c | d | e |',
          '| a || b |\n| --- | --- |\n| c |',
          '| a \\| b | c |\n| --- | --- |\n| d \\| e | f',
          '| |\n| - |\n| x | y | z |',
        ];

        for (final doc in docs) {
          final controller = SovereignController(
            text: doc,
            syntaxEngine: const V1SyntaxEngineAdapter(),
          );
          addTearDown(controller.dispose);

          final offsets = List<int>.generate(
            controller.text.length + 1,
            (i) => i,
          );
          for (final offset in offsets) {
            controller.selection = TextSelection.collapsed(offset: offset);
            expect(
              () => controller.handleTabKey(reverse: false),
              returnsNormally,
              reason: 'forward tab threw at offset=$offset for doc=$doc',
            );
            _expectControllerInvariants(
              controller,
              seed: 0,
              step: offset,
              fixture: doc,
            );
            expect(
              () => controller.handleTabKey(reverse: true),
              returnsNormally,
              reason: 'reverse tab threw at offset=$offset for doc=$doc',
            );
            _expectControllerInvariants(
              controller,
              seed: 0,
              step: offset + 1000,
              fixture: doc,
            );
          }
        }
      },
    );
  });
}

void _insertRandomChar(SovereignController controller, Random rand) {
  const alphabet = <String>[
    'a',
    'b',
    'c',
    ' ',
    '\n',
    '|',
    '-',
    ':',
    '\\',
    '>',
    '`',
    '*',
    '_',
    '[',
    ']',
    '(',
    ')',
  ];

  final ch = alphabet[rand.nextInt(alphabet.length)];
  final text = controller.text;
  final sel = controller.selection;
  final caret = _collapsedCaret(sel, text.length);
  final newText = text.replaceRange(caret, caret, ch);
  controller.value = TextEditingValue(
    text: newText,
    selection: TextSelection.collapsed(offset: caret + ch.length),
    composing: TextRange.empty,
  );
}

void _replaceSelectionWithRandomChar(
  SovereignController controller,
  Random rand,
) {
  final text = controller.text;
  if (text.isEmpty) {
    _insertRandomChar(controller, rand);
    return;
  }

  final start = rand.nextInt(text.length + 1);
  final end = start + rand.nextInt(text.length - start + 1);
  controller.selection = TextSelection(baseOffset: start, extentOffset: end);
  _insertRandomChar(controller, rand);
}

void _backspace(SovereignController controller) {
  final text = controller.text;
  final sel = controller.selection;
  if (sel.isValid && !sel.isCollapsed) {
    final start = sel.start.clamp(0, text.length);
    final end = sel.end.clamp(0, text.length);
    final newText = text.replaceRange(start, end, '');
    controller.value = TextEditingValue(
      text: newText,
      selection: TextSelection.collapsed(offset: start),
      composing: TextRange.empty,
    );
    return;
  }

  final caret = _collapsedCaret(sel, text.length);
  if (caret <= 0) return;

  final newText = text.replaceRange(caret - 1, caret, '');
  controller.value = TextEditingValue(
    text: newText,
    selection: TextSelection.collapsed(offset: caret - 1),
    composing: TextRange.empty,
  );
}

void _moveCaretRandom(SovereignController controller, Random rand) {
  final len = controller.text.length;
  if (len == 0) {
    controller.selection = const TextSelection.collapsed(offset: 0);
    return;
  }
  final offset = rand.nextInt(len + 1);
  controller.selection = TextSelection.collapsed(offset: offset);
}

void _toggleRandomComposingRange(SovereignController controller, Random rand) {
  final text = controller.text;
  final sel = controller.selection;
  final caret = _collapsedCaret(sel, text.length);
  if (text.isEmpty || rand.nextBool()) {
    controller.value = controller.value.copyWith(composing: TextRange.empty);
    return;
  }

  final start = max(0, min(caret, text.length - 1));
  final end = min(text.length, start + 1);
  controller.value = controller.value.copyWith(
    selection: TextSelection.collapsed(offset: end),
    composing: TextRange(start: start, end: end),
  );
}

int _collapsedCaret(TextSelection sel, int textLength) {
  if (!sel.isValid) return textLength;
  final offset = sel.isCollapsed ? sel.baseOffset : sel.extentOffset;
  return offset.clamp(0, textLength);
}

void _expectControllerInvariants(
  SovereignController controller, {
  required int seed,
  required int step,
  required String fixture,
}) {
  final text = controller.text;
  final sel = controller.selection;

  expect(
    sel.isValid,
    isTrue,
    reason: 'seed=$seed step=$step invalid selection for fixture=$fixture',
  );
  expect(
    sel.start >= 0 && sel.start <= text.length,
    isTrue,
    reason:
        'seed=$seed step=$step selection.start out of bounds for fixture=$fixture',
  );
  expect(
    sel.end >= 0 && sel.end <= text.length,
    isTrue,
    reason:
        'seed=$seed step=$step selection.end out of bounds for fixture=$fixture',
  );

  final composing = controller.value.composing;
  if (composing.isValid) {
    expect(
      composing.start >= 0 && composing.start <= text.length,
      isTrue,
      reason:
          'seed=$seed step=$step composing.start out of bounds for fixture=$fixture',
    );
    expect(
      composing.end >= 0 && composing.end <= text.length,
      isTrue,
      reason:
          'seed=$seed step=$step composing.end out of bounds for fixture=$fixture',
    );
    expect(
      composing.start <= composing.end,
      isTrue,
      reason: 'seed=$seed step=$step composing inverted for fixture=$fixture',
    );
  }

  final hidden = controller.decoration.hiddenRanges;
  var prevEnd = 0;
  for (final range in hidden) {
    expect(
      range.start >= 0,
      isTrue,
      reason: 'seed=$seed step=$step hidden.start < 0: $range fixture=$fixture',
    );
    expect(
      range.end > range.start,
      isTrue,
      reason:
          'seed=$seed step=$step hidden empty/invalid: $range fixture=$fixture',
    );
    expect(
      range.end <= text.length,
      isTrue,
      reason:
          'seed=$seed step=$step hidden.end out of bounds: $range fixture=$fixture',
    );
    expect(
      range.start >= prevEnd,
      isTrue,
      reason:
          'seed=$seed step=$step hidden overlap/unsorted: $hidden fixture=$fixture',
    );
    prevEnd = range.end;
  }
}
