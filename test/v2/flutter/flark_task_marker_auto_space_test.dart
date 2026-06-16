import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';

void main() {
  group('Task marker auto-space', () {
    bool hasCheckbox(FlarkFlutterController c) => c.renderPlan.blocks
        .expand((b) => [b, ...b.children])
        .any((b) => b.taskListItem != null);

    test('completing the marker inserts the trailing space', () async {
      for (final probe in <(String, String, int)>[
        ('- [ ', '- [ ] ', 6),
        ('- [x', '- [x] ', 6),
        ('* [ ', '* [ ] ', 6),
        ('  - [ ', '  - [ ] ', 8),
      ]) {
        final c = FlarkFlutterController.fromMarkdown(probe.$1);
        addTearDown(c.dispose);
        await c.parseNow();
        c.applySelection(
          FlarkSelection.collapsed(probe.$1.length),
          userEvent: 't',
        );
        final old = c.projection.projectText(c.markdown);
        c.applyProjectedTextEdit(oldDisplayText: old, newDisplayText: '$old]');

        expect(c.markdown, probe.$2, reason: 'completing "${probe.$1}]"');
        // Caret lands after the inserted space, ready for content.
        expect(c.selection, FlarkSelection.collapsed(probe.$3));
      }
    });

    test('content typed after the marker stays in the checkbox', () async {
      final c = FlarkFlutterController.fromMarkdown('- [ ');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(4), userEvent: 't');

      final old = c.projection.projectText(c.markdown);
      c.applyProjectedTextEdit(oldDisplayText: old, newDisplayText: '$old]');
      expect(c.markdown, '- [ ] ');

      await c.parseNow();
      final old2 = c.projection.projectText(c.markdown);
      c.applyProjectedTextEdit(oldDisplayText: old2, newDisplayText: '${old2}f');

      expect(c.markdown, '- [ ] f');
      await c.parseNow();
      expect(hasCheckbox(c), isTrue);
    });

    test('an armed wrap flags an immediate parse so the checkbox renders',
        () async {
      final c = FlarkFlutterController.fromMarkdown('- [ ');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(4), userEvent: 't');
      final old = c.projection.projectText(c.markdown);
      c.applyProjectedTextEdit(oldDisplayText: old, newDisplayText: '$old]');
      expect(c.lastEditRequestsImmediateParse, isTrue);
    });

    test('ordinary typing is unaffected', () async {
      final c = FlarkFlutterController.fromMarkdown('ab');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(1), userEvent: 't');
      c.applyProjectedTextEdit(oldDisplayText: 'ab', newDisplayText: 'axb');
      expect(c.markdown, 'axb');
    });

    test('a marker that already has its space is not doubled', () async {
      // Inserting a content char into `- [ ] ` must not add another space.
      final c = FlarkFlutterController.fromMarkdown('- [ ] ');
      addTearDown(c.dispose);
      await c.parseNow();
      c.applySelection(const FlarkSelection.collapsed(6), userEvent: 't');
      final old = c.projection.projectText(c.markdown);
      c.applyProjectedTextEdit(oldDisplayText: old, newDisplayText: '${old}x');
      expect(c.markdown, '- [ ] x');
    });
  });
}
