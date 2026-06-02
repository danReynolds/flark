import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';
import 'package:sovereign_editor/src/v2/projection/projection.dart';

void main() {
  testWidgets('renders projected text and applies edits to source markdown', (
    tester,
  ) async {
    final controller = SovereignFlutterController(
      runtime: SovereignEditorRuntime(
        state: SovereignEditorState.fromMarkdown(
          '**bold**',
          selection: const SovereignSelection.collapsed(6),
        ),
      ),
      projection: _boldProjection(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SovereignProjectedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    var editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'bold');
    expect(
      editable.controller.selection,
      const TextSelection.collapsed(offset: 4),
    );

    await tester.enterText(find.byType(EditableText), 'bold!');
    await tester.pump();

    expect(controller.markdown, '**bold!**');
    editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.controller.text, 'bold!');
  });

  testWidgets('groups projected IME composition updates into one undo step', (
    tester,
  ) async {
    final controller = SovereignFlutterController(
      runtime: SovereignEditorRuntime(
        state: SovereignEditorState.fromMarkdown(
          '**bold**',
          selection: const SovereignSelection.collapsed(6),
        ),
      ),
      projection: _boldProjection(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SovereignProjectedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    var editable = tester.widget<EditableText>(find.byType(EditableText));
    editable.controller.value = const TextEditingValue(
      text: 'a',
      selection: TextSelection.collapsed(offset: 1),
      composing: TextRange(start: 0, end: 1),
    );
    await tester.pump();

    editable = tester.widget<EditableText>(find.byType(EditableText));
    editable.controller.value = const TextEditingValue(
      text: 'あ',
      selection: TextSelection.collapsed(offset: 1),
      composing: TextRange(start: 0, end: 1),
    );
    await tester.pump();

    editable = tester.widget<EditableText>(find.byType(EditableText));
    editable.controller.value = const TextEditingValue(
      text: 'あ',
      selection: TextSelection.collapsed(offset: 1),
    );
    await tester.pump();

    expect(controller.markdown, '**あ**');
    controller.undo();
    await tester.pump();
    expect(controller.markdown, '**bold**');
  });

  testWidgets('maps projected selection updates back to source offsets', (
    tester,
  ) async {
    final controller = SovereignFlutterController(
      runtime: SovereignEditorRuntime(
        state: SovereignEditorState.fromMarkdown(
          '**bold**',
          selection: const SovereignSelection.collapsed(6),
        ),
      ),
      projection: _boldProjection(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SovereignProjectedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    editable.controller.selection = const TextSelection.collapsed(offset: 0);
    await tester.pump();

    expect(controller.selection, const SovereignSelection.collapsed(2));
  });

  testWidgets('merges partial style with ambient text defaults', (
    tester,
  ) async {
    final controller = SovereignFlutterController(
      runtime: SovereignEditorRuntime(
        state: SovereignEditorState.fromMarkdown('**bold**'),
      ),
      projection: _boldProjection(),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: DefaultTextStyle(
          style: const TextStyle(color: Color(0xFF17202A), fontSize: 18),
          child: SovereignProjectedEditableText(
            controller: controller,
            style: const TextStyle(fontFamily: 'monospace'),
          ),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.style.color, const Color(0xFF17202A));
    expect(editable.style.fontSize, 18);
    expect(editable.style.fontFamily, 'monospace');
  });

  testWidgets('can expand to fill a document pane', (tester) async {
    final controller = SovereignFlutterController.fromMarkdown('');
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SizedBox(
          height: 240,
          child: SovereignProjectedEditableText(
            controller: controller,
            expands: true,
            maxLines: null,
          ),
        ),
      ),
    );

    final editable = tester.widget<EditableText>(find.byType(EditableText));
    expect(editable.expands, isTrue);
    expect(editable.minLines, isNull);
    expect(editable.maxLines, isNull);
    expect(editable.textInputAction, TextInputAction.newline);
  });

  testWidgets('routes Enter through markdown input policy', (tester) async {
    final controller = SovereignFlutterController.fromMarkdown(
      '- item',
      extensions: SovereignExtensionSet([
        const SovereignMarkdownInputEditingExtension(),
      ]),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SovereignProjectedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    await tester.enterText(find.byType(EditableText), '- item\n');
    await tester.pump();

    expect(controller.markdown, '- item\n- ');
    expect(controller.selection, const SovereignSelection.collapsed(9));
  });

  testWidgets('routes Backspace through markdown input policy', (tester) async {
    final controller = SovereignFlutterController.fromMarkdown(
      '- item',
      extensions: SovereignExtensionSet([
        const SovereignMarkdownInputEditingExtension(),
      ]),
    );
    addTearDown(controller.dispose);

    await tester.pumpWidget(
      Directionality(
        textDirection: TextDirection.ltr,
        child: SovereignProjectedEditableText(
          controller: controller,
          style: const TextStyle(fontSize: 14),
        ),
      ),
    );

    controller.applySelection(
      const SovereignSelection.collapsed(2),
      userEvent: 'test',
    );
    await tester.pump();

    await tester.enterText(find.byType(EditableText), '-item');
    await tester.pump();

    expect(controller.markdown, 'item');
    expect(controller.selection, const SovereignSelection.collapsed(0));
  });

  testWidgets(
    'routes hardware Backspace at projected quote text start through policy',
    (tester) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '> quote',
        extensions: SovereignMarkdownEditingExtensions.standard(),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);
      controller.applySelection(
        const SovereignSelection.collapsed(2),
        userEvent: 'test',
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignProjectedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'quote');
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );

      await tester.showKeyboard(find.byType(EditableText));
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(controller.markdown, 'quote');
      expect(controller.selection, const SovereignSelection.collapsed(0));
    },
  );

  testWidgets(
    'routes hardware Backspace at projected heading text start through policy',
    (tester) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '## Heading',
        extensions: SovereignMarkdownEditingExtensions.standard(),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);
      controller.applySelection(
        const SovereignSelection.collapsed(3),
        userEvent: 'test',
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignProjectedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'Heading');
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );

      await tester.showKeyboard(find.byType(EditableText));
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(controller.markdown, 'Heading');
      expect(controller.selection, const SovereignSelection.collapsed(0));
    },
  );

  testWidgets(
    'routes hardware Backspace at projected list text start through policy',
    (tester) async {
      final controller = SovereignFlutterController.fromMarkdown(
        '- item',
        extensions: SovereignMarkdownEditingExtensions.standard(),
      );
      addTearDown(controller.dispose);
      await _applyComrakParseResult(controller);
      controller.applySelection(
        const SovereignSelection.collapsed(2),
        userEvent: 'test',
      );

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SovereignProjectedEditableText(
            controller: controller,
            style: const TextStyle(fontSize: 14),
            autofocus: true,
          ),
        ),
      );
      await tester.pump();

      final editable = tester.widget<EditableText>(find.byType(EditableText));
      expect(editable.controller.text, 'item');
      expect(
        editable.controller.selection,
        const TextSelection.collapsed(offset: 0),
      );

      await tester.showKeyboard(find.byType(EditableText));
      await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
      await tester.pump();

      expect(controller.markdown, 'item');
      expect(controller.selection, const SovereignSelection.collapsed(0));
    },
  );
}

SovereignProjection _boldProjection() {
  return SovereignProjection(
    textLength: 8,
    hiddenRanges: const [
      SovereignHiddenRange(
        range: SovereignSourceRange(0, 2),
        kind: SovereignHiddenRangeKind.inlineMarker,
      ),
      SovereignHiddenRange(
        range: SovereignSourceRange(6, 8),
        kind: SovereignHiddenRangeKind.inlineMarker,
      ),
    ],
  );
}

Future<void> _applyComrakParseResult(
  SovereignFlutterController controller,
) async {
  final result = await SovereignNativeComrakParseBackend.withNativeBridge()
      .parse(
        SovereignMarkdownParseRequest(
          revision: controller.state.revision,
          markdown: controller.markdown,
          profile: SovereignMarkdownProfile.commonMarkGfm,
        ),
      );
  expect(controller.applyParseResult(result), isTrue);
}
