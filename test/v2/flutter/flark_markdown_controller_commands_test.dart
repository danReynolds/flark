import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

void main() {
  test('controller markdown helpers wrap common toolbar commands', () {
    final controller = FlarkFlutterController.fromMarkdown('hello world');
    addTearDown(controller.dispose);

    controller.applySelection(
      const FlarkSelection(baseOffset: 0, extentOffset: 5),
    );

    final commands = controller.commands;
    expect(commands.strongActive, isFalse);
    expect(commands.canUndo, isFalse);

    expect(commands.toggleStrong().commandResult.isHandled, isTrue);
    expect(controller.markdown, '**hello** world');
    expect(commands.strongActive, isTrue);
    expect(commands.canUndo, isTrue);

    controller.applySelection(
      FlarkSelection.collapsed(controller.markdown.length),
    );
    expect(
      commands.insertCodeFence(language: 'dart').commandResult.isHandled,
      isTrue,
    );
    expect(controller.markdown, '**hello** world\n\n```dart\n\n```');

    final tableController = FlarkFlutterController.fromMarkdown('');
    addTearDown(tableController.dispose);
    expect(
      tableController.commands
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
    final controller = FlarkFlutterController.fromMarkdown('title\nlink');
    addTearDown(controller.dispose);

    controller.applySelection(const FlarkSelection.collapsed(0));
    final commands = controller.commands;
    expect(commands.setHeadingLevel(2).commandResult.isHandled, isTrue);
    expect(controller.markdown, '## title\nlink');
    expect(commands.headingLevel, 2);

    controller.applySelection(
      const FlarkSelection(baseOffset: 9, extentOffset: 13),
    );
    final linkContext = commands.resolveLinkEditContext();
    expect(linkContext.label, 'link');
    expect(
      commands
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
      const FlarkSelection(baseOffset: 9, extentOffset: 36),
    );
    final existingLink = commands.resolveLinkEditContext();
    expect(existingLink.isExisting, isTrue);
    expect(
      commands
          .removeLink(linkRange: existingLink.replaceRange)
          .commandResult
          .isHandled,
      isTrue,
    );
    expect(controller.markdown, '## title\nDocs');

    controller.applySelection(
      const FlarkSelection(baseOffset: 0, extentOffset: 5),
    );
    expect(commands.toggleQuote().commandResult.isHandled, isTrue);
    expect(controller.markdown, '> ## title\nDocs');
    expect(commands.quoteActive, isTrue);
  });
}
