import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  // Regression guard for per-block rebuild isolation (Stages 1-3): an unchanged
  // block must reuse its widget instance (skipping the rebuild) when it is
  // byte-identical in content, position, and selection. A no-shift edit is the
  // case where every other block qualifies, so only the edited block rebuilds.
  // (Insertions that shift later blocks' offsets still rebuild those blocks —
  // see docs/architecture/live_rendered_rebuild_isolation.md.)
  testWidgets('a no-shift edit rebuilds only the edited block', (tester) async {
    final backend = FlarkNativeComrakParseBackend.tryLoad();
    if (backend == null) return; // Native bridge required for a real plan.

    const markdown =
        '- [ ] alpha\n- [ ] bravo\n- [ ] charlie\n- [ ] delta\n- [ ] echo\n'
        '- [ ] foxtrot';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    final parsed = await tester.runAsync(
      () => backend.parse(
        FlarkMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: markdown,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      ),
    );
    expect(controller.applyParseResult(parsed!), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    final fields = find.byType(EditableText);
    final blockCount = fields.evaluate().length;
    expect(blockCount, greaterThanOrEqualTo(6));

    // A same-length, in-place edit in the first block shifts no offsets, so
    // every other block is byte-identical in content, position, and selection
    // and must be reused (skipped) — only the edited block rebuilds.
    flarkDebugLiveBlockBuildCount = 0;
    await tester.enterText(fields.first, 'alphX'); // 'alpha' -> 'alphX'
    await tester.pump();

    expect(
      flarkDebugLiveBlockBuildCount,
      lessThanOrEqualTo(2),
      reason:
          'a no-shift edit should rebuild only the edited block, not '
          '$flarkDebugLiveBlockBuildCount of $blockCount',
    );
  });

  testWidgets('an offset-shifting edit reuses unchanged later blocks', (
    tester,
  ) async {
    final backend = FlarkNativeComrakParseBackend.tryLoad();
    if (backend == null) return; // Native bridge required for a real plan.

    const markdown =
        '- [ ] alpha\n- [ ] bravo\n- [ ] charlie\n- [ ] delta\n- [ ] echo\n'
        '- [ ] foxtrot';
    final controller = FlarkFlutterController.fromMarkdown(markdown);
    addTearDown(controller.dispose);
    final parsed = await tester.runAsync(
      () => backend.parse(
        FlarkMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: markdown,
          profile: FlarkMarkdownProfile.commonMarkGfm,
        ),
      ),
    );
    expect(controller.applyParseResult(parsed!), isTrue);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: FlarkLiveRenderedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );
    await tester.pump();

    final fields = find.byType(EditableText);
    final blockCount = fields.evaluate().length;
    expect(blockCount, greaterThanOrEqualTo(6));

    // Lengthening the first block shifts every later block's source/display
    // ranges. Unchanged later blocks should still reuse their widget instances.
    flarkDebugLiveBlockBuildCount = 0;
    await tester.enterText(fields.first, 'alphaX');
    await tester.pump();

    expect(
      flarkDebugLiveBlockBuildCount,
      lessThanOrEqualTo(2),
      reason:
          'offset shifts alone should not force all later blocks to rebuild',
    );

    // The reused second block must still resolve its current shifted source
    // range when edited; otherwise this corrupts the first item or marker text.
    await tester.enterText(fields.at(1), 'bravY');
    await tester.pump();

    expect(
      controller.markdown,
      '- [ ] alphaX\n- [ ] bravY\n- [ ] charlie\n- [ ] delta\n'
      '- [ ] echo\n- [ ] foxtrot',
    );
  });
}
