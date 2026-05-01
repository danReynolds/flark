import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';
import 'package:sovereign_editor/widgets/sovereign/engine/v1_syntax_engine_adapter.dart';

void main() {
  group('Sovereign interaction fuzz invariants', () {
    test(
      'Quote/list/fence-heavy mixed operations preserve controller invariants',
      () {
        const seeds = <int>[11, 31, 57];
        const fixtures = <String>[
          '> alpha\n> \nnext',
          '- item\n- [ ] task\n1. ordered',
          '> - item\n> - \n> 1. ordered',
          '> - [ ] task\n> 1. [x] done\n> > nested quote',
          '> > - [ ] nested task\n> > \n> tail',
          '> 1. [ ] one\n> 2. [x] two\noutside',
          '```\n- list-ish\n> quote-ish\n```',
          '> ```\n> code\n> ```\n> tail',
          '- item\n\n```\ncode\n```\n> quote',
          '> - item\n> ```\n> code\n> ```\n> - tail',
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

            for (var step = 0; step < 100; step++) {
              final action = rand.nextInt(13);
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
                  controller.handleArrowDownKey();
                  break;
                case 4:
                  controller.handleArrowUpKey();
                  break;
                case 5:
                  controller.handleTabKey(reverse: false);
                  break;
                case 6:
                  controller.handleTabKey(reverse: true);
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
                case 10:
                  if (rand.nextBool()) {
                    controller.undo();
                  } else {
                    controller.redo();
                  }
                  break;
                case 11:
                  _setRandomSelection(controller, rand);
                  break;
                case 12:
                  controller.toggleTaskCheckboxAtSelection();
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
      },
    );
  });
}

void _insertRandomChar(SovereignController controller, Random rand) {
  const alphabet = <String>[
    'a',
    'b',
    ' ',
    '\n',
    '`',
    '*',
    '_',
    '>',
    '-',
    '.',
    '[',
    ']',
    ':',
    '|',
    '\\',
    '(',
    ')',
  ];

  final ch = alphabet[rand.nextInt(alphabet.length)];
  final text = controller.text;
  final sel = controller.selection;
  final start = sel.isValid ? sel.start.clamp(0, text.length) : text.length;
  final end = sel.isValid ? sel.end.clamp(0, text.length) : text.length;
  final newText = text.replaceRange(start, end, ch);
  controller.value = TextEditingValue(
    text: newText,
    selection: TextSelection.collapsed(offset: start + ch.length),
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
  controller.selection = TextSelection.collapsed(
    offset: len == 0 ? 0 : rand.nextInt(len + 1),
  );
}

void _setRandomSelection(SovereignController controller, Random rand) {
  final len = controller.text.length;
  if (len == 0) {
    controller.selection = const TextSelection.collapsed(offset: 0);
    return;
  }
  final base = rand.nextInt(len + 1);
  final extent = rand.nextInt(len + 1);
  controller.selection = TextSelection(baseOffset: base, extentOffset: extent);
}

void _toggleRandomComposingRange(SovereignController controller, Random rand) {
  final text = controller.text;
  if (text.isEmpty || rand.nextBool()) {
    controller.value = controller.value.copyWith(composing: TextRange.empty);
    return;
  }

  final start = rand.nextInt(text.length);
  final end = min(text.length, start + 1 + rand.nextInt(2));
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
    reason: 'seed=$seed step=$step invalid selection fixture=$fixture',
  );
  expect(
    sel.start >= 0 && sel.start <= text.length,
    isTrue,
    reason: 'seed=$seed step=$step sel.start OOB fixture=$fixture',
  );
  expect(
    sel.end >= 0 && sel.end <= text.length,
    isTrue,
    reason: 'seed=$seed step=$step sel.end OOB fixture=$fixture',
  );

  final composing = controller.value.composing;
  if (composing.isValid) {
    expect(
      composing.start >= 0 && composing.start <= text.length,
      isTrue,
      reason: 'seed=$seed step=$step composing.start OOB fixture=$fixture',
    );
    expect(
      composing.end >= 0 && composing.end <= text.length,
      isTrue,
      reason: 'seed=$seed step=$step composing.end OOB fixture=$fixture',
    );
    expect(
      composing.start <= composing.end,
      isTrue,
      reason: 'seed=$seed step=$step composing inverted fixture=$fixture',
    );
  }

  final hidden = controller.decoration.hiddenRanges;
  var prevEnd = 0;
  for (final range in hidden) {
    expect(
      range.start >= 0,
      isTrue,
      reason: 'seed=$seed step=$step hidden.start<0 $range fixture=$fixture',
    );
    expect(
      range.end > range.start,
      isTrue,
      reason: 'seed=$seed step=$step hidden invalid $range fixture=$fixture',
    );
    expect(
      range.end <= text.length,
      isTrue,
      reason: 'seed=$seed step=$step hidden.end OOB $range fixture=$fixture',
    );
    expect(
      range.start >= prevEnd,
      isTrue,
      reason: 'seed=$seed step=$step hidden overlap $hidden fixture=$fixture',
    );
    prevEnd = range.end;
  }
}
