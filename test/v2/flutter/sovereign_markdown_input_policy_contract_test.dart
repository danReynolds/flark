import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/flutter/flutter.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('Sovereign markdown input policy contract', () {
    for (final surface in _EditingSurface.values) {
      for (final contractCase in _backspaceCases) {
        testWidgets(
          '${surface.label}: Backspace at ${contractCase.label} boundary',
          (tester) async {
            final controller = SovereignFlutterController.fromMarkdown(
              contractCase.markdown,
              extensions: SovereignMarkdownEditingExtensions.standard(),
            );
            addTearDown(controller.dispose);
            await _pumpSurface(tester, surface, controller);
            controller.applySelection(
              SovereignSelection.collapsed(contractCase.caret),
              userEvent: 'test',
            );
            await tester.pump();

            final editable = tester.widget<EditableText>(_editableFinder());
            expect(
              editable.controller.text,
              surface == _EditingSurface.source
                  ? contractCase.markdown
                  : contractCase.visibleText,
            );
            expect(
              editable.controller.selection,
              TextSelection.collapsed(
                offset:
                    surface == _EditingSurface.source ? contractCase.caret : 0,
              ),
            );

            await tester.showKeyboard(_editableFinder());
            await tester.sendKeyEvent(LogicalKeyboardKey.backspace);
            await _settle(tester);

            expect(controller.markdown, contractCase.expectedMarkdown);
            expect(
              controller.selection,
              SovereignSelection.collapsed(contractCase.expectedCaret),
            );
          },
        );
      }

      testWidgets('${surface.label}: Enter continues quote structure',
          (tester) async {
        final controller = SovereignFlutterController.fromMarkdown(
          '> quote',
          extensions: SovereignMarkdownEditingExtensions.standard(),
        );
        addTearDown(controller.dispose);
        await _pumpSurface(tester, surface, controller);
        controller.applySelection(
          const SovereignSelection.collapsed(7),
          userEvent: 'test',
        );
        await tester.pump();

        await tester.showKeyboard(_editableFinder());
        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await _settle(tester);

        expect(controller.markdown, '> quote\n> ');
        expect(controller.selection, const SovereignSelection.collapsed(10));
      });

      testWidgets('${surface.label}: Shift+Enter inserts a soft line break',
          (tester) async {
        final controller = SovereignFlutterController.fromMarkdown(
          '- item',
          extensions: SovereignMarkdownEditingExtensions.standard(),
        );
        addTearDown(controller.dispose);
        await _pumpSurface(tester, surface, controller);
        controller.applySelection(
          const SovereignSelection.collapsed(6),
          userEvent: 'test',
        );
        await tester.pump();

        await tester.showKeyboard(_editableFinder());
        await tester.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
        await tester.sendKeyEvent(LogicalKeyboardKey.enter);
        await tester.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
        await _settle(tester);

        expect(controller.markdown, '- item\n');
        expect(controller.selection, const SovereignSelection.collapsed(7));
      });
    }
  });
}

Finder _editableFinder() => find.byType(EditableText).first;

Future<void> _pumpSurface(
  WidgetTester tester,
  _EditingSurface surface,
  SovereignFlutterController controller,
) async {
  if (surface != _EditingSurface.source) {
    await _applyComrakParseResult(controller);
  }
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: switch (surface) {
        _EditingSurface.source => SovereignEditableText(
            controller: controller,
            autofocus: true,
            style: const TextStyle(fontSize: 14),
          ),
        _EditingSurface.projected => SovereignProjectedEditableText(
            controller: controller,
            autofocus: true,
            style: const TextStyle(fontSize: 14),
          ),
        _EditingSurface.liveRendered => SovereignLiveRenderedEditableText(
            controller: controller,
            autofocus: true,
            style: const TextStyle(fontSize: 14),
          ),
      },
    ),
  );
  await _settle(tester);
}

Future<void> _applyComrakParseResult(
  SovereignFlutterController controller,
) async {
  final result =
      await SovereignNativeComrakParseBackend.withNativeBridge().parse(
    SovereignMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: SovereignMarkdownProfile.commonMarkGfm,
    ),
  );
  expect(controller.applyParseResult(result), isTrue);
}

Future<void> _settle(WidgetTester tester) async {
  await tester.pump();
  await tester.pump(const Duration(milliseconds: 200));
  await tester.pump(const Duration(milliseconds: 200));
}

enum _EditingSurface {
  source('source'),
  projected('projected'),
  liveRendered('live rendered');

  const _EditingSurface(this.label);

  final String label;
}

final class _BackspaceBoundaryCase {
  const _BackspaceBoundaryCase({
    required this.label,
    required this.markdown,
    required this.visibleText,
    required this.caret,
    required this.expectedMarkdown,
    required this.expectedCaret,
  });

  final String label;
  final String markdown;
  final String visibleText;
  final int caret;
  final String expectedMarkdown;
  final int expectedCaret;
}

const _backspaceCases = [
  _BackspaceBoundaryCase(
    label: 'quote text start',
    markdown: '> quote',
    visibleText: 'quote',
    caret: 2,
    expectedMarkdown: 'quote',
    expectedCaret: 0,
  ),
  _BackspaceBoundaryCase(
    label: 'heading text start',
    markdown: '## Heading',
    visibleText: 'Heading',
    caret: 3,
    expectedMarkdown: 'Heading',
    expectedCaret: 0,
  ),
  _BackspaceBoundaryCase(
    label: 'unordered list text start',
    markdown: '- item',
    visibleText: 'item',
    caret: 2,
    expectedMarkdown: 'item',
    expectedCaret: 0,
  ),
  _BackspaceBoundaryCase(
    label: 'ordered list text start',
    markdown: '1. item',
    visibleText: 'item',
    caret: 3,
    expectedMarkdown: 'item',
    expectedCaret: 0,
  ),
  _BackspaceBoundaryCase(
    label: 'task text start',
    markdown: '- [x] done',
    visibleText: 'done',
    caret: 6,
    expectedMarkdown: '- done',
    expectedCaret: 2,
  ),
];
