import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flark_live_block_source_edit.dart';
import 'package:flark/src/v2/flutter/flark_live_code_fence_input_policy.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('FlarkLiveCodeFenceInputPolicy', () {
    test('normalizes repeated platform line-break echoes to one break', () {
      const oldText = 'body\n';
      final value = TextEditingValue(
        text: '$oldText\n\n',
        selection: const TextSelection.collapsed(offset: 7),
      );

      final normalized =
          FlarkLiveCodeFenceInputPolicy.normalizeLineBreakInsertionValue(
            block: _codeBlock('```\n$oldText```'),
            oldText: oldText,
            value: value,
          );

      expect(normalized, isNotNull);
      expect(normalized!.text, 'body\n\n');
      expect(normalized.selection, const TextSelection.collapsed(offset: 6));
      expect(normalized.composing, TextRange.empty);
    });

    test('completes a standalone fence opener with a body line', () {
      final text =
          FlarkLiveCodeFenceInputPolicy.displayTextAfterCompletingStandaloneOpener(
            oldDisplayText: '``',
            oldSelection: const TextSelection.collapsed(offset: 2),
            newValue: const TextEditingValue(
              text: '```',
              selection: TextSelection.collapsed(offset: 3),
            ),
          );

      expect(text, '```\n');
    });

    test('moves continued typing after a completed opener into the body', () {
      final value =
          FlarkLiveCodeFenceInputPolicy.valueAfterCompletingStandaloneOpener(
            oldDisplayText: '``',
            oldSelection: const TextSelection.collapsed(offset: 2),
            newValue: const TextEditingValue(
              text: '```fffff',
              selection: TextSelection.collapsed(offset: 8),
            ),
          );

      expect(value, isNotNull);
      expect(value!.text, '```\nfffff');
      expect(value.selection, const TextSelection.collapsed(offset: 9));
      expect(value.composing, TextRange.empty);
    });

    test('moves batched text after a new opener into the body', () {
      final value =
          FlarkLiveCodeFenceInputPolicy.valueAfterCompletingStandaloneOpener(
            oldDisplayText: '',
            oldSelection: const TextSelection.collapsed(offset: 0),
            newValue: const TextEditingValue(
              text: '```fffff',
              selection: TextSelection.collapsed(offset: 8),
            ),
          );

      expect(value, isNotNull);
      expect(value!.text, '```\nfffff');
      expect(value.selection, const TextSelection.collapsed(offset: 9));
    });

    test('normalizes standalone auto-close platform echoes', () {
      final markdown =
          FlarkLiveCodeFenceInputPolicy.markdownAfterAutoClosedStandaloneEcho(
            oldMarkdown: '```',
            newValue: const TextEditingValue(
              text: '```\n```\n',
              selection: TextSelection.collapsed(offset: 8),
            ),
          );

      expect(markdown, '```\n');
    });

    test(
      'normalizes whole-value auto-close echoes before projection mapping',
      () {
        final text =
            FlarkLiveCodeFenceInputPolicy.displayTextAfterAutoClosedWholeValueEcho(
              const TextEditingValue(
                text: '```\n```',
                selection: TextSelection.collapsed(offset: 7),
              ),
            );

        expect(text, '```\n');
      },
    );

    test(
      'consumes a pending body echo while the source snapshot catches up',
      () {
        const markdown = '```\nbody\n```';
        const pendingText = 'body\n';
        final value = TextEditingValue(
          text: '$pendingText\n',
          selection: const TextSelection.collapsed(offset: pendingText.length),
        );

        final decision = FlarkLiveCodeFenceInputPolicy.consumePendingEcho(
          pendingText: pendingText,
          markdown: markdown,
          block: _codeBlock(markdown),
          value: value,
        );

        expect(decision.consumed, isTrue);
        expect(decision.nextPendingText, isNull);
      },
    );

    test('promotes recognized body language shortcuts into fence metadata', () {
      const markdown = '```\ndart';
      final range = FlarkSourceRange(4, markdown.length);
      final value = TextEditingValue(
        text: 'dart\n',
        selection: const TextSelection.collapsed(offset: 5),
      );

      final edit = FlarkLiveCodeFenceInputPolicy.languageShortcutEdit(
        markdown: markdown,
        block: _codeBlock(markdown),
        range: range,
        oldText: 'dart',
        value: value,
      );

      expect(edit, isA<FlarkLiveBlockSourceEdit>());
      expect(edit!.range, FlarkSourceRange(0, markdown.length));
      expect(edit.replacementText, '```dart\n');
      expect(edit.editableRangeAfter, const FlarkSourceRange(8, 8));
      expect(edit.selectionAfter, const FlarkSelection.collapsed(8));
    });

    test('promotes coalesced language shortcuts and preserves body text', () {
      const markdown = '```\njs';
      final range = FlarkSourceRange(4, markdown.length);
      final value = TextEditingValue(
        text: 'js\nconsole.log(1);',
        selection: const TextSelection.collapsed(offset: 18),
      );

      final edit = FlarkLiveCodeFenceInputPolicy.languageShortcutEdit(
        markdown: markdown,
        block: _codeBlock(markdown),
        range: range,
        oldText: 'js',
        value: value,
      );

      expect(edit, isA<FlarkLiveBlockSourceEdit>());
      expect(edit!.range, FlarkSourceRange(0, markdown.length));
      expect(edit.replacementText, '```javascript\nconsole.log(1);');
      expect(edit.editableRangeAfter, const FlarkSourceRange(14, 29));
      expect(edit.selectionAfter, const FlarkSelection.collapsed(29));
    });

    test('leaves unrecognized body text in the code body', () {
      const markdown = '```\ngggg';
      final range = FlarkSourceRange(4, markdown.length);
      final value = TextEditingValue(
        text: 'gggg\n',
        selection: const TextSelection.collapsed(offset: 5),
      );

      final edit = FlarkLiveCodeFenceInputPolicy.languageShortcutEdit(
        markdown: markdown,
        block: _codeBlock(markdown),
        range: range,
        oldText: 'gggg',
        value: value,
      );

      expect(edit, isNull);
    });
  });
}

FlarkRenderBlock _codeBlock(String markdown, {String? language}) {
  return FlarkRenderBlock(
    kind: FlarkMarkdownBlockKind.codeBlock,
    type: 'codeBlock',
    sourceRange: FlarkSourceRange(0, markdown.length),
    displayRange: FlarkSourceRange(0, markdown.length),
    styleToken: FlarkRenderTextStyleToken.body,
    inlineRuns: const [],
    children: const [],
    codeBlock: FlarkRenderCodeBlockDescriptor(language: language),
  );
}
