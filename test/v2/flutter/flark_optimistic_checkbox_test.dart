import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Optimistic checkbox (forming task marker)', () {
    bool hasCheckbox(FlarkFlutterController c) {
      return c.renderPlan.blocks
          .expand((b) => [b, ...b.children])
          .any((b) => b.taskListItem != null);
    }

    bool? checked(FlarkFlutterController c) {
      for (final b in c.renderPlan.blocks.expand((b) => [b, ...b.children])) {
        if (b.taskListItem != null) return b.taskListItem!.checked;
      }
      return null;
    }

    test('renders a checkbox at each forming state under the caret', () async {
      for (final probe in <(String, int, bool)>[
        ('- [', 3, false),
        ('- [ ', 4, false),
        ('- [ ]', 5, false),
        ('- [x', 4, true),
        ('- [x]', 5, true),
        ('- [X]', 5, true),
        ('* [ ', 4, false), // other bullet markers
        ('  - [ ', 6, false), // indented
      ]) {
        final c = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(c.dispose);
        await c.parseNow();
        c.applySelection(FlarkSelection.collapsed(probe.$2), userEvent: 't');
        await c.parseNow();

        expect(hasCheckbox(c), isTrue, reason: 'checkbox for "${probe.$1}"');
        expect(checked(c), probe.$3, reason: 'checked for "${probe.$1}"');
        // The bracket token is hidden — the item renders as an empty checkbox.
        expect(c.projection.projectText(c.markdown).trim(), '');
      }
    });

    test('an incomplete token with text after it stays literal', () async {
      // `- [ task` has no closing `]` and trailing content, so it is a plain
      // bullet with literal `[ task`, not a forming checkbox.
      for (final src in <String>['- [ task', '- [x without close', '- task']) {
        final c = FlarkFlutterController.fromMarkdown(src);
        addTearDown(c.dispose);
        await c.parseNow();
        c.applySelection(
          FlarkSelection.collapsed(src.length),
          userEvent: 't',
        );
        await c.parseNow();
        expect(hasCheckbox(c), isFalse, reason: '"$src" is not a forming checkbox');
      }
    });

    test('a completed "- [ ] " is owned by the parser (real task)', () async {
      final c = FlarkFlutterController.fromMarkdown('- [ ] real');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(0), userEvent: 't');
      await c.parseNow();
      // Even with the caret out of the item, a real task still renders.
      expect(hasCheckbox(c), isTrue);
      expect(c.projection.projectText(c.markdown), 'real');
    });

    test('only forms a checkbox while the caret is in the item', () async {
      // The optimistic render is for the marker the caret is actively typing;
      // a forming `- [ ` the caret is not in renders as a plain bullet. (Like
      // the sticky-inline-run pass, it re-reconciles on parse, so this is the
      // caret position at adoption.)
      final inItem = FlarkFlutterController.fromMarkdown('hello\n\n- [ ');
      addTearDown(inItem.dispose);
      inItem.applySelection(const FlarkSelection.collapsed(11), userEvent: 't');
      await inItem.parseNow();
      expect(hasCheckbox(inItem), isTrue, reason: 'caret inside the forming item');

      final outside = FlarkFlutterController.fromMarkdown('hello\n\n- [ ');
      addTearDown(outside.dispose);
      outside.applySelection(const FlarkSelection.collapsed(2), userEvent: 't');
      await outside.parseNow();
      expect(hasCheckbox(outside), isFalse, reason: 'caret in "hello", not the item');
    });

    test('forming a task marker flags an immediate parse', () async {
      final c = FlarkFlutterController.fromMarkdown('- ');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(2), userEvent: 't');

      c.applyProjectedTextEdit(oldDisplayText: '', newDisplayText: '[');
      expect(c.markdown, '- [');
      expect(c.lastEditRequestsImmediateParse, isTrue);
    });

    test('plain list typing does not flag an immediate parse', () async {
      final c = FlarkFlutterController.fromMarkdown('- ');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(2), userEvent: 't');

      c.applyProjectedTextEdit(oldDisplayText: '', newDisplayText: 'a');
      expect(c.markdown, '- a');
      expect(c.lastEditRequestsImmediateParse, isFalse);
    });
  });
}
