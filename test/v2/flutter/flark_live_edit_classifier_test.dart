import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flark_projected_editable_text.dart';
import 'package:flark/src/v2/markdown/markdown.dart';
import 'package:flark/src/v2/render_plan/render_plan.dart';

void main() {
  group('classifyFlarkLiveBlockEdit', () {
    test('plain text change with a direct source range replaces source', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: '```dart\nfoo\n```',
          block: _codeBlock('```dart\nfoo\n```'),
          sourceRange: const FlarkSourceRange(8, 11),
          oldText: 'foo',
          oldSelection: const TextSelection.collapsed(offset: 3),
          newValue: const TextEditingValue(
            text: 'foob',
            selection: TextSelection.collapsed(offset: 4),
          ),
        ),
      );

      final intent = classification.intent;
      expect(intent, isA<FlarkLiveBlockPlatformTextChangeIntent>());
      final wrapped = intent as FlarkLiveBlockPlatformTextChangeIntent;
      expect(wrapped.resyncWhenHandled, isFalse);
      expect(
        wrapped.fallback,
        isA<FlarkLiveBlockDirectReplacementIntent>().having(
          (i) => i.sourceRange,
          'sourceRange',
          const FlarkSourceRange(8, 11),
        ),
      );
    });

    test('typed closing fence bypasses the markdown input policy', () {
      // 'foo\n```' inside an unclosed fence body must reach the direct
      // replacement unmediated, so the typed closer lands in the source.
      const markdown = '```dart\nfoo';
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: markdown,
          block: _codeBlock(markdown),
          sourceRange: const FlarkSourceRange(8, 11),
          oldText: 'foo',
          oldSelection: const TextSelection.collapsed(offset: 3),
          newValue: const TextEditingValue(
            text: 'foo\n```',
            selection: TextSelection.collapsed(offset: 7),
          ),
        ),
      );

      expect(
        classification.intent,
        isA<FlarkLiveBlockDirectReplacementIntent>(),
      );
    });

    test('non-policy surfaces never wrap in a platform-text-change offer', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: 'hello',
          block: _paragraph('hello'),
          sourceRange: null,
          oldText: 'hello',
          oldSelection: const TextSelection.collapsed(offset: 5),
          newValue: const TextEditingValue(
            text: 'hello!',
            selection: TextSelection.collapsed(offset: 6),
          ),
          markdownInputPolicyEnabled: false,
        ),
      );

      final intent = classification.intent;
      expect(intent, isA<FlarkLiveBlockProjectedEditIntent>());
      final projected = intent as FlarkLiveBlockProjectedEditIntent;
      expect(projected.newDisplayText, 'hello!');
      expect(projected.adoptBlockValue, isFalse);
      expect(projected.immediateParseAfterApply, isFalse);
    });

    test('pure insertion normalization moves the caret to the insert end', () {
      // Some platforms deliver an insertion with the caret still at the
      // insertion point; the normalized value places it after the text.
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: 'ab',
          block: _paragraph('ab'),
          sourceRange: null,
          oldText: 'ab',
          oldSelection: const TextSelection.collapsed(offset: 1),
          newValue: const TextEditingValue(
            text: 'aXb',
            selection: TextSelection.collapsed(offset: 1),
          ),
          markdownInputPolicyEnabled: false,
        ),
      );

      expect(
        classification.normalizedValue.selection,
        const TextSelection.collapsed(offset: 2),
      );
    });

    test('selection-only change inside a source range maps to source', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: '```dart\nfoo\n```',
          block: _codeBlock('```dart\nfoo\n```'),
          sourceRange: const FlarkSourceRange(8, 11),
          oldText: 'foo',
          oldSelection: const TextSelection.collapsed(offset: 1),
          newValue: const TextEditingValue(
            text: 'foo',
            selection: TextSelection.collapsed(offset: 2),
          ),
        ),
      );

      final intent = classification.intent;
      expect(intent, isA<FlarkLiveBlockSourceSelectionIntent>());
      final selection = intent as FlarkLiveBlockSourceSelectionIntent;
      expect(selection.selection, const FlarkSelection.collapsed(10));
      expect(selection.snapshotRange, const FlarkSourceRange(8, 11));
    });

    test('selection-only change without a source range maps via display', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: 'hello',
          block: _paragraph('hello', displayStart: 4),
          displayText: 'pre\nhello',
          sourceRange: null,
          oldText: 'hello',
          oldSelection: const TextSelection.collapsed(offset: 0),
          newValue: const TextEditingValue(
            text: 'hello',
            selection: TextSelection.collapsed(offset: 3),
          ),
          markdownInputPolicyEnabled: false,
        ),
      );

      final intent = classification.intent;
      expect(intent, isA<FlarkLiveBlockProjectedSelectionIntent>());
      expect(
        (intent as FlarkLiveBlockProjectedSelectionIntent).selection,
        const FlarkSelection.collapsed(7),
      );
    });

    test('invalid selections classify as ignore', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: 'hello',
          block: _paragraph('hello'),
          sourceRange: null,
          oldText: 'hello',
          oldSelection: const TextSelection.collapsed(offset: 5),
          newValue: const TextEditingValue(text: 'hello'),
          markdownInputPolicyEnabled: false,
        ),
      );

      expect(classification.intent, isA<FlarkLiveBlockIgnoreIntent>());
    });

    test('pending code-body echo is consumed exactly once', () {
      const markdown = '```\nbody\n```';
      const pendingText = 'body\n';
      final block = _codeBlock(markdown);
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: markdown,
          block: block,
          sourceRange: const FlarkSourceRange(4, 9),
          oldText: pendingText,
          oldSelection: const TextSelection.collapsed(offset: 0),
          newValue: const TextEditingValue(
            text: '$pendingText\n',
            selection: TextSelection.collapsed(offset: pendingText.length),
          ),
          pendingCodeBodyEchoText: pendingText,
        ),
      );

      expect(
        classification.intent,
        isA<FlarkLiveBlockResyncIntent>().having(
          (i) => i.reason,
          'reason',
          FlarkLiveBlockResyncReason.pendingCodeBodyEcho,
        ),
      );
      expect(classification.nextPendingCodeBodyEchoText, isNull);
    });

    test('the pending echo slot is untouched when no probe runs', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: 'hello',
          block: _paragraph('hello'),
          sourceRange: null,
          oldText: 'hello',
          oldSelection: const TextSelection.collapsed(offset: 5),
          newValue: const TextEditingValue(
            text: 'hello',
            selection: TextSelection.collapsed(offset: 2),
          ),
          markdownInputPolicyEnabled: false,
          pendingCodeBodyEchoText: 'stale',
        ),
      );

      expect(classification.nextPendingCodeBodyEchoText, 'stale');
    });

    test('a completed standalone fence opener rewrites the block value', () {
      final classification = classifyFlarkLiveBlockEdit(
        _blockContext(
          markdown: '``',
          block: _paragraph('``'),
          sourceRange: null,
          oldText: '``',
          oldSelection: const TextSelection.collapsed(offset: 2),
          newValue: const TextEditingValue(
            text: '```',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
      );

      FlarkLiveBlockEditIntent intent = classification.intent;
      if (intent is FlarkLiveBlockPlatformTextChangeIntent) {
        intent = intent.fallback;
      }
      expect(intent, isA<FlarkLiveBlockProjectedEditIntent>());
      final projected = intent as FlarkLiveBlockProjectedEditIntent;
      expect(projected.adoptBlockValue, isTrue);
      expect(projected.blockValue.text, '```\n');
      expect(projected.immediateParseAfterApply, isTrue);
    });
  });

  group('classifyFlarkHostEdit', () {
    test('text change wraps a projected edit in a policy offer', () {
      final classification = classifyFlarkHostEdit(
        _hostContext(
          markdown: 'hello',
          oldDisplayText: 'hello',
          newValue: const TextEditingValue(
            text: 'hello!',
            selection: TextSelection.collapsed(offset: 6),
          ),
        ),
      );

      final intent = classification.intent;
      expect(intent, isA<FlarkHostPlatformTextChangeIntent>());
      final wrapped = intent as FlarkHostPlatformTextChangeIntent;
      expect(wrapped.oldText, 'hello');
      expect(wrapped.fallback.newDisplayText, 'hello!');
      expect(wrapped.fallback.immediateParseAfterApply, isFalse);
    });

    test('an immediately renderable line requests an immediate parse', () {
      final classification = classifyFlarkHostEdit(
        _hostContext(
          markdown: 'hello',
          oldDisplayText: 'hello',
          newValue: const TextEditingValue(
            text: 'hello\n- ',
            selection: TextSelection.collapsed(offset: 8),
          ),
        ),
      );

      final intent = classification.intent as FlarkHostPlatformTextChangeIntent;
      expect(intent.fallback.immediateParseAfterApply, isTrue);
    });

    test('non-live hosts never request an immediate parse', () {
      final classification = classifyFlarkHostEdit(
        _hostContext(
          markdown: 'hello',
          oldDisplayText: 'hello',
          newValue: const TextEditingValue(
            text: 'hello\n- ',
            selection: TextSelection.collapsed(offset: 8),
          ),
          liveRendered: false,
        ),
      );

      final intent = classification.intent as FlarkHostPlatformTextChangeIntent;
      expect(intent.fallback.immediateParseAfterApply, isFalse);
    });

    test('selection-only change classifies as projected selection', () {
      final classification = classifyFlarkHostEdit(
        _hostContext(
          markdown: 'hello',
          oldDisplayText: 'hello',
          newValue: const TextEditingValue(
            text: 'hello',
            selection: TextSelection.collapsed(offset: 3),
          ),
        ),
      );

      expect(
        classification.intent,
        isA<FlarkHostProjectedSelectionIntent>().having(
          (i) => i.selection,
          'selection',
          const FlarkSelection.collapsed(3),
        ),
      );
    });

    test('invalid selections classify as ignore', () {
      final classification = classifyFlarkHostEdit(
        _hostContext(
          markdown: 'hello',
          oldDisplayText: 'hello',
          newValue: const TextEditingValue(text: 'hello'),
        ),
      );

      expect(classification.intent, isA<FlarkHostIgnoreIntent>());
    });
  });
}

FlarkLiveBlockEditContext _blockContext({
  required String markdown,
  required FlarkRenderBlock block,
  required FlarkSourceRange? sourceRange,
  required String oldText,
  required TextSelection oldSelection,
  required TextEditingValue newValue,
  String? displayText,
  bool markdownInputPolicyEnabled = true,
  String? pendingCodeBodyEchoText,
}) {
  return FlarkLiveBlockEditContext(
    markdown: markdown,
    block: block,
    displayText: displayText ?? markdown,
    sourceRange: sourceRange,
    oldValue: TextEditingValue(text: oldText, selection: oldSelection),
    newValue: newValue,
    markdownInputPolicyEnabled: markdownInputPolicyEnabled,
    pendingCodeBodyEchoText: pendingCodeBodyEchoText,
  );
}

FlarkHostEditContext _hostContext({
  required String markdown,
  required String oldDisplayText,
  required TextEditingValue newValue,
  bool liveRendered = true,
}) {
  return FlarkHostEditContext(
    markdown: markdown,
    oldDisplayText: oldDisplayText,
    oldDisplaySelection: FlarkSelection.collapsed(oldDisplayText.length),
    newValue: newValue,
    liveRendered: liveRendered,
  );
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

FlarkRenderBlock _paragraph(String text, {int displayStart = 0}) {
  return FlarkRenderBlock(
    kind: FlarkMarkdownBlockKind.paragraph,
    type: 'paragraph',
    sourceRange: FlarkSourceRange(0, text.length),
    displayRange: FlarkSourceRange(displayStart, displayStart + text.length),
    styleToken: FlarkRenderTextStyleToken.body,
    inlineRuns: const [],
    children: const [],
  );
}
