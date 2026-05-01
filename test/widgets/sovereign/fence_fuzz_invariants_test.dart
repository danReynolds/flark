import 'dart:math';

import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/widgets/sovereign/controllers/sovereign_controller.dart';

void main() {
  group('Sovereign fence fuzz invariants', () {
    test('Mixed edit/navigation operations preserve controller invariants', () {
      final seeds = <int>[7, 17, 29, 41, 53];
      for (final seed in seeds) {
        final controller = SovereignController(text: '```\n\n```');
        addTearDown(controller.dispose);
        controller.selection = const TextSelection.collapsed(offset: 4);

        final rand = Random(seed);
        for (var i = 0; i < 140; i++) {
          final action = rand.nextInt(7);
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
          }

          _expectControllerInvariants(controller, seed: seed, step: i);
        }
      }
    });
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
    '{',
    '}',
    '[',
    ']',
    ':',
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
  );
}

void _backspace(SovereignController controller) {
  final text = controller.text;
  final sel = controller.selection;
  final caret = _collapsedCaret(sel, text.length);
  if (caret <= 0) return;

  final newText = text.replaceRange(caret - 1, caret, '');
  controller.value = TextEditingValue(
    text: newText,
    selection: TextSelection.collapsed(offset: caret - 1),
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
}) {
  final text = controller.text;
  final sel = controller.selection;

  expect(
    sel.isValid,
    isTrue,
    reason: 'seed=$seed step=$step selection became invalid',
  );
  expect(
    sel.start >= 0 && sel.start <= text.length,
    isTrue,
    reason: 'seed=$seed step=$step selection.start out of bounds',
  );
  expect(
    sel.end >= 0 && sel.end <= text.length,
    isTrue,
    reason: 'seed=$seed step=$step selection.end out of bounds',
  );

  final hidden = controller.decoration.hiddenRanges;
  var prevEnd = 0;
  for (final range in hidden) {
    expect(
      range.start >= 0,
      isTrue,
      reason: 'seed=$seed step=$step hidden range start < 0: $range',
    );
    expect(
      range.end > range.start,
      isTrue,
      reason: 'seed=$seed step=$step hidden range is empty/invalid: $range',
    );
    expect(
      range.end <= text.length,
      isTrue,
      reason: 'seed=$seed step=$step hidden range end out of bounds: $range',
    );
    expect(
      range.start >= prevEnd,
      isTrue,
      reason:
          'seed=$seed step=$step hidden ranges overlap or are unsorted: $hidden',
    );
    prevEnd = range.end;
  }
}
