import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

void main() {
  group('SovereignMarkdownCommands table commands', () {
    test('insertTable inserts a source-aligned parser-backed GFM table',
        () async {
      final controller = SovereignController(
        text: 'alpha',
        markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
      );
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 5);

      final result = controller.commands.insertTable();

      expect(result, isA<SovereignCommandApplied>());
      expect(
        controller.text,
        equals(
          'alpha\n\n'
          '| Header 1 | Header 2 |\n'
          '| -------- | -------- |\n'
          '|          |          |\n',
        ),
      );
      final bodyRowStart = controller.text.lastIndexOf(
        '|          |          |',
      );
      expect(
        controller.selection,
        TextSelection.collapsed(offset: bodyRowStart + 2),
      );

      await _eventually(() {
        return controller.decoration.tree.blocks.any(
          (block) => block.type == BlockType.table,
        );
      });
    });

    test('insertTableRowBelow inserts and aligns an empty body row', () {
      final controller = SovereignController(
        text: '| A | B |\n| - | - |\n| x | y |',
      );
      addTearDown(controller.dispose);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('x'),
      );

      final result = controller.commands.insertTableRowBelow();

      expect(result, isA<SovereignCommandApplied>());
      expect(
        controller.text,
        equals(
          '| A   | B   |\n'
          '| --- | --- |\n'
          '| x   | y   |\n'
          '|     |     |',
        ),
      );
      final insertedRowStart = controller.text.lastIndexOf('|     |     |');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: insertedRowStart + 2),
      );
    });

    test('insertTableColumnRight inserts and aligns an empty column', () {
      final controller = SovereignController(
        text: '| A | B |\n| - | - |\n| x | y |',
      );
      addTearDown(controller.dispose);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('x'),
      );

      final result = controller.commands.insertTableColumnRight();

      expect(result, isA<SovereignCommandApplied>());
      expect(
        controller.text,
        equals(
          '| A   |     | B   |\n'
          '| --- | --- | --- |\n'
          '| x   |     | y   |',
        ),
      );
      final bodyRowStart = controller.text.lastIndexOf('| x');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: bodyRowStart + 8),
      );
    });

    test('deleteTableColumn removes the current column and keeps two columns',
        () {
      final controller = SovereignController(
        text: '| A | B | C |\n| - | - | - |\n| x | y | z |',
      );
      addTearDown(controller.dispose);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('y'),
      );

      final result = controller.commands.deleteTableColumn();

      expect(result, isA<SovereignCommandApplied>());
      expect(
        controller.text,
        equals(
          '| A   | C   |\n'
          '| --- | --- |\n'
          '| x   | z   |',
        ),
      );
      final bodyRowStart = controller.text.lastIndexOf('| x');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: bodyRowStart + 8),
      );
    });

    test('deleteTableRow removes the current body row', () {
      final controller = SovereignController(
        text: '| A | B |\n| - | - |\n| x | y |\n| q | r |',
      );
      addTearDown(controller.dispose);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('x'),
      );

      final result = controller.commands.deleteTableRow();

      expect(result, isA<SovereignCommandApplied>());
      expect(
        controller.text,
        equals(
          '| A   | B   |\n'
          '| --- | --- |\n'
          '| q   | r   |',
        ),
      );
      final bodyRowStart = controller.text.lastIndexOf('| q');
      expect(
        controller.selection,
        TextSelection.collapsed(offset: bodyRowStart + 2),
      );
    });

    test('table row and column commands no-op outside established tables', () {
      final controller = SovereignController(text: 'not | a | table');
      addTearDown(controller.dispose);
      controller.selection = const TextSelection.collapsed(offset: 6);

      final row = controller.commands.insertTableRowBelow();
      final column = controller.commands.insertTableColumnRight();

      expect(row, isA<SovereignCommandNoOp>());
      expect(column, isA<SovereignCommandNoOp>());
      expect(controller.text, 'not | a | table');
    });

    test('table row and column commands no-op inside fenced code', () {
      const source = '```\n| A | B |\n| - | - |\n```';
      final controller = SovereignController(text: source);
      addTearDown(controller.dispose);
      controller.selection = TextSelection.collapsed(
        offset: controller.text.indexOf('A'),
      );

      final row = controller.commands.insertTableRowBelow();
      final column = controller.commands.insertTableColumnRight();

      expect(row, isA<SovereignCommandNoOp>());
      expect(column, isA<SovereignCommandNoOp>());
      expect(controller.text, source);
    });

    test('table commands reject while IME composing', () {
      final controller = SovereignController(text: '| A | B |');
      addTearDown(controller.dispose);
      controller.value = const TextEditingValue(
        text: '| A | B |',
        selection: TextSelection.collapsed(offset: 2),
        composing: TextRange(start: 0, end: 2),
      );

      final result = controller.commands.insertTable();

      expect(result, isA<SovereignCommandRejected>());
      expect(
        (result as SovereignCommandRejected).reasonCode,
        SovereignCommandReasonCode.imeComposing,
      );
      expect(controller.text, '| A | B |');
    });
  });
}

Future<void> _eventually(bool Function() predicate, {int turns = 80}) async {
  for (var i = 0; i < turns; i++) {
    if (predicate()) return;
    await Future<void>.delayed(Duration.zero);
    await Future<void>.delayed(Duration.zero);
    if (i % 5 == 4) {
      await Future<void>.delayed(const Duration(milliseconds: 1));
    }
  }
  expect(predicate(), isTrue);
}
