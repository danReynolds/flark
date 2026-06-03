import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  // Regression guard for per-block rebuild isolation. Skipped until the
  // node-view rearchitecture lands: today every block widget rebuilds on every
  // keystroke (measured 6/6) because block editables receive the whole-document
  // displayText and absolute offsets and re-sync from the parent rebuild rather
  // than self-resolving their own slice. Remove `skip` when blocks become
  // position-independent.
  testWidgets('editing one live block does not rebuild every block', (
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

    // Type into the first block and count how many block widgets rebuild.
    flarkDebugLiveBlockBuildCount = 0;
    await tester.enterText(fields.first, 'alpha!');
    await tester.pump();

    // ignore: avoid_print
    print('REBUILD_FANOUT blocks=$blockCount '
        'builds=$flarkDebugLiveBlockBuildCount');

    // Editing one block should rebuild a bounded number of blocks, not all of
    // them. Allow a small constant for the edited block and its neighbors.
    expect(
      flarkDebugLiveBlockBuildCount,
      lessThanOrEqualTo(3),
      reason: 'editing one block rebuilt $flarkDebugLiveBlockBuildCount of '
          '$blockCount block widgets',
    );
    // skip: enable when per-block rebuild isolation lands (node-view rewrite).
  }, skip: true);
}
