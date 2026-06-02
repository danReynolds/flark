import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

void main() {
  test('controller markdown helpers wrap common toolbar commands', () {
    final controller = SovereignFlutterController.fromMarkdown('hello world');
    addTearDown(controller.dispose);

    controller.applySelection(
      const SovereignSelection(baseOffset: 0, extentOffset: 5),
    );

    expect(controller.toggleStrong().commandResult.isHandled, isTrue);
    expect(controller.markdown, '**hello** world');

    controller.applySelection(
      SovereignSelection.collapsed(controller.markdown.length),
    );
    expect(
      controller.insertCodeFence(language: 'dart').commandResult.isHandled,
      isTrue,
    );
    expect(controller.markdown, '**hello** world\n\n```dart\n\n```');

    final tableController = SovereignFlutterController.fromMarkdown('');
    addTearDown(tableController.dispose);
    expect(
      tableController
          .insertTable(columns: 3, bodyRows: 2)
          .commandResult
          .isHandled,
      isTrue,
    );
    expect(
      tableController.markdown,
      contains('| Header 1 | Header 2 | Header 3 |'),
    );
  });

  test('controller markdown helpers expose block and link editing', () {
    final controller = SovereignFlutterController.fromMarkdown('title\nlink');
    addTearDown(controller.dispose);

    controller.applySelection(const SovereignSelection.collapsed(0));
    expect(controller.setHeadingLevel(2).commandResult.isHandled, isTrue);
    expect(controller.markdown, '## title\nlink');

    controller.applySelection(
      const SovereignSelection(baseOffset: 9, extentOffset: 13),
    );
    final linkContext = controller.resolveLinkEditContext();
    expect(linkContext.label, 'link');
    expect(
      controller
          .applyLinkEdit(
            context: linkContext,
            label: 'Docs',
            url: 'https://example.com',
          )
          .commandResult
          .isHandled,
      isTrue,
    );
    expect(controller.markdown, '## title\n[Docs](https://example.com)');

    controller.applySelection(
      const SovereignSelection(baseOffset: 9, extentOffset: 36),
    );
    final existingLink = controller.resolveLinkEditContext();
    expect(existingLink.isExisting, isTrue);
    expect(
      controller
          .removeLink(linkRange: existingLink.replaceRange)
          .commandResult
          .isHandled,
      isTrue,
    );
    expect(controller.markdown, '## title\nDocs');

    controller.applySelection(
      const SovereignSelection(baseOffset: 0, extentOffset: 5),
    );
    expect(controller.toggleQuote().commandResult.isHandled, isTrue);
    expect(controller.markdown, '> ## title\nDocs');
  });
}
