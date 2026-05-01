import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  group('SovereignMarkdownCommands link commands', () {
    test('resolveLinkEditContext detects existing link around caret', () {
      final controller = SovereignController(text: 'See [docs](https://a.b).');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 8);

      final context = controller.commands.resolveLinkEditContext();

      expect(context.isExisting, isTrue);
      expect(context.label, equals('docs'));
      expect(context.url, equals('https://a.b'));
      expect(context.replaceRange.start, equals(4));
      expect(context.replaceRange.end, equals(23));
    });

    test('resolveLinkEditContext returns selected text as new label', () {
      final controller = SovereignController(text: 'hello world');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 5,
      );

      final context = controller.commands.resolveLinkEditContext();

      expect(context.isExisting, isFalse);
      expect(context.label, equals('hello'));
      expect(context.url, equals('https://'));
      expect(context.replaceRange, const TextRange(start: 0, end: 5));
    });

    test('resolveLinkEditContext detects exact selected markdown link', () {
      const text = '[docs](https://a.b)';
      final controller = SovereignController(text: text);
      addTearDown(controller.dispose);
      controller.selection = TextSelection(
        baseOffset: 0,
        extentOffset: text.length,
      );

      final context = controller.commands.resolveLinkEditContext();

      expect(context.isExisting, isTrue);
      expect(context.label, equals('docs'));
      expect(context.url, equals('https://a.b'));
      expect(context.replaceRange, TextRange(start: 0, end: text.length));
    });

    test('applyLinkEdit replaces existing link in place', () {
      final controller = SovereignController(text: 'See [docs](https://a.b).');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 8);
      final context = controller.commands.resolveLinkEditContext();

      final result = controller.commands.applyLinkEdit(
        context: context,
        label: 'guide',
        url: 'https://example.com',
      );

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('See [guide](https://example.com).'));
      expect(
        controller.selection,
        const TextSelection.collapsed(
          offset: 'See [guide](https://example.com)'.length,
        ),
      );
    });

    test('applyLinkEdit inserts new link at collapsed caret', () {
      final controller = SovereignController(text: 'See ');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);
      final context = controller.commands.resolveLinkEditContext();

      final result = controller.commands.applyLinkEdit(
        context: context,
        label: 'docs',
        url: 'https://example.com',
      );

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('See [docs](https://example.com)'));
      expect(
        controller.selection,
        const TextSelection.collapsed(
          offset: 'See [docs](https://example.com)'.length,
        ),
      );
    });

    test('applyLinkEdit returns no-op for empty url', () {
      final controller = SovereignController(text: 'See ');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);
      final context = controller.commands.resolveLinkEditContext();

      final result = controller.commands.applyLinkEdit(
        context: context,
        label: 'docs',
        url: '   ',
      );

      expect(result, isA<SovereignCommandNoOp>());
      expect(
        (result as SovereignCommandNoOp).reasonCode,
        SovereignCommandReasonCode.emptyUrl,
      );
      expect(controller.text, equals('See '));
    });

    test('applyLinkEdit defaults blank label to "link"', () {
      final controller = SovereignController(text: 'See ');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);
      final context = controller.commands.resolveLinkEditContext();

      final result = controller.commands.applyLinkEdit(
        context: context,
        label: '   ',
        url: 'https://example.com',
      );

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('See [link](https://example.com)'));
    });

    test('insertLink inserts empty-url markdown link and selects url', () {
      final controller = SovereignController(text: 'See ');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 4);

      final result = controller.commands.insertLink();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('See [link text]()'));
      expect(
        controller.selection,
        const TextSelection(baseOffset: 17, extentOffset: 17),
      );
    });

    test('insertLink uses selected text as link label', () {
      final controller = SovereignController(text: 'hello world');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection(
        baseOffset: 0,
        extentOffset: 5,
      );

      final result = controller.commands.insertLink();

      expect(result, isA<SovereignCommandApplied>());
      expect(controller.text, equals('[hello]() world'));
      expect(
        controller.selection,
        const TextSelection(baseOffset: 9, extentOffset: 9),
      );
    });
  });
}
