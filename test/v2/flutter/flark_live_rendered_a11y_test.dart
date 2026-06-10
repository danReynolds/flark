import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  testWidgets('live task checkbox meets tap-target and exposes checkbox '
      'semantics', (tester) async {
    final handle = tester.ensureSemantics();
    final controller = FlarkFlutterController.fromMarkdown('- [ ] Write tests');
    addTearDown(controller.dispose);
    expect(controller.applyParseResult(_taskParseResult(controller)), isTrue);

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

    // The interactive checkbox must be a 48x48 tap target.
    await expectLater(tester, meetsGuideline(androidTapTargetGuideline));

    // Screen readers must see a checkbox with a label and an unchecked state.
    expect(
      tester.getSemantics(find.byKey(const Key('FlarkLiveBlockTaskCheckbox'))),
      isSemantics(
        hasCheckedState: true,
        isChecked: false,
        hasTapAction: true,
        label: 'Task, not completed',
      ),
    );

    handle.dispose();
  });

  testWidgets('toggled live task checkbox reports checked state to semantics', (
    tester,
  ) async {
    final handle = tester.ensureSemantics();
    final controller = FlarkFlutterController.fromMarkdown('- [x] Done');
    addTearDown(controller.dispose);
    expect(
      controller.applyParseResult(_taskParseResult(controller, checked: true)),
      isTrue,
    );

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

    expect(
      tester.getSemantics(find.byKey(const Key('FlarkLiveBlockTaskCheckbox'))),
      isSemantics(
        hasCheckedState: true,
        isChecked: true,
        label: 'Task, completed',
      ),
    );

    handle.dispose();
  });
}

FlarkMarkdownParseResult _taskParseResult(
  FlarkFlutterController controller, {
  bool checked = false,
}) {
  return FlarkMarkdownParseResult(
    schemaVersion: FlarkMarkdownParseProtocol.currentSchemaVersion,
    revision: controller.state.revision,
    sourceTextLength: controller.markdown.length,
    blocks: [
      FlarkMarkdownBlockNode(
        kind: FlarkMarkdownBlockKind.listItem,
        type: 'listItem',
        sourceRange: FlarkSourceRange(0, controller.markdown.length),
        attributes: {'checked': checked},
      ),
    ],
    inlineTokens: const [],
    hiddenRanges: [
      FlarkMarkdownHiddenRange(
        kind: FlarkMarkdownHiddenRangeKind.blockMarker,
        type: 'blockMarker',
        sourceRange: const FlarkSourceRange(0, 6),
      ),
    ],
  );
}
