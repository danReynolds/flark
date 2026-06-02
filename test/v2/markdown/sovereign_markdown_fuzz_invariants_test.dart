import 'dart:math';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

import '../support/sovereign_test_paths.dart';

void main() {
  group('Sovereign markdown editing fuzz invariants', () {
    for (final seed in [11, 29, 47, 83]) {
      test('mixed editing session stays internally consistent for seed $seed',
          () {
        var runtime = SovereignEditorRuntime.fromMarkdown(
          '',
          extensions: SovereignMarkdownEditingExtensions.standard(),
        );
        final random = Random(seed);

        for (var step = 0; step < 240; step++) {
          try {
            runtime = _nextRuntime(runtime, random);
            _expectRuntimeInvariants(runtime);
          } catch (error, stackTrace) {
            fail(
              'Seed $seed failed at step $step with markdown '
              '${runtime.state.markdown}: $error\n$stackTrace',
            );
          }
        }
      });
    }
  });

  group('Sovereign parser-backed markdown editing fuzz invariants', () {
    final libPath = sovereignNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; parser-backed fuzz suite skipped', () {
        debugPrint(
          'Skipped parser-backed markdown fuzz: native bridge missing.',
        );
        expect(true, isTrue);
      });
      return;
    }

    final backend = SovereignNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    for (final seed in [101, 202, 303]) {
      test('mixed live-edit parse adoption stays consistent for seed $seed',
          () async {
        final controller = SovereignFlutterController.fromMarkdown(
          '',
          extensions: SovereignMarkdownEditingExtensions.standard(),
        );
        addTearDown(controller.dispose);
        final random = Random(seed);

        for (var step = 0; step < 120; step++) {
          try {
            _nextControllerState(controller, random);
            await _expectParserBackedControllerInvariants(
              controller,
              backend,
            );
          } catch (error, stackTrace) {
            fail(
              'Seed $seed failed at step $step with markdown '
              '${controller.markdown}: $error\n$stackTrace',
            );
          }
        }
      });
    }
  });
}

SovereignEditorRuntime _nextRuntime(
  SovereignEditorRuntime runtime,
  Random random,
) {
  final action = random.nextInt(12);
  return switch (action) {
    0 || 1 || 2 || 3 => _insertRandomText(runtime, random),
    4 => _dispatchEnter(runtime),
    5 => _dispatchBackspace(runtime),
    6 => _applyRandomSelection(runtime, random),
    7 => runtime.canUndo ? runtime.undo().runtime : runtime,
    8 => runtime.canRedo ? runtime.redo().runtime : runtime,
    9 => _replaceRandomRange(runtime, random),
    10 => _toggleInlineStrong(runtime),
    _ => _toggleBlockShape(runtime, random),
  };
}

SovereignEditorRuntime _insertRandomText(
  SovereignEditorRuntime runtime,
  Random random,
) {
  const samples = [
    'a',
    ' ',
    '>',
    '-',
    '*',
    '1',
    '.',
    '[',
    ']',
    '`',
    '\n',
    'foo',
    '```',
  ];
  return runtime
      .dispatch(
        command: SovereignCoreEditingCommands.insertText,
        payload: SovereignInsertTextPayload(
          samples[random.nextInt(samples.length)],
        ),
      )
      .runtime;
}

SovereignEditorRuntime _dispatchEnter(SovereignEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: SovereignMarkdownInputCommands.handleEnter,
        payload: const SovereignHandleEnterPayload(),
      )
      .runtime;
}

SovereignEditorRuntime _dispatchBackspace(SovereignEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: SovereignMarkdownInputCommands.handleBackspace,
        payload: const SovereignHandleBackspacePayload(),
      )
      .runtime;
}

SovereignEditorRuntime _applyRandomSelection(
  SovereignEditorRuntime runtime,
  Random random,
) {
  final length = runtime.state.markdown.length;
  final base = random.nextInt(length + 1);
  final extent = random.nextInt(length + 1);
  return runtime
      .applyTransaction(
        SovereignTransaction(
          operations: const [],
          selectionAfter: SovereignSelection(
            baseOffset: base,
            extentOffset: extent,
          ),
          metadata: const SovereignTransactionMetadata(
            intent: SovereignTransactionIntent.selection,
            userEvent: 'test.fuzz.selection',
            addToHistory: false,
          ),
        ),
      )
      .runtime;
}

SovereignEditorRuntime _replaceRandomRange(
  SovereignEditorRuntime runtime,
  Random random,
) {
  final length = runtime.state.markdown.length;
  final a = random.nextInt(length + 1);
  final b = random.nextInt(length + 1);
  final start = min(a, b);
  final end = max(a, b);
  const replacements = ['', '\n', 'text', '> ', '- ', '```dart\n'];
  final replacement = replacements[random.nextInt(replacements.length)];
  return runtime
      .applyTransaction(
        SovereignTransaction.single(
          SovereignSourceOperation.replace(
            replacedRange: SovereignSourceRange(start, end),
            replacementText: replacement,
          ),
          selectionAfter: SovereignSelection.collapsed(
            start + replacement.length,
          ),
          metadata: SovereignTransactionMetadata(
            intent: SovereignTransactionIntent.input,
            userEvent: 'test.fuzz.replace',
            parseInvalidationRange: SovereignSourceRange(start, end),
            projectionInvalidationRange: SovereignSourceRange(start, end),
          ),
        ),
      )
      .runtime;
}

SovereignEditorRuntime _toggleInlineStrong(SovereignEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      )
      .runtime;
}

SovereignEditorRuntime _toggleBlockShape(
  SovereignEditorRuntime runtime,
  Random random,
) {
  final command = random.nextInt(4);
  return switch (command) {
    0 => runtime
        .dispatch(
          command: SovereignMarkdownBlockCommands.toggleBulletList,
          payload: const SovereignToggleBulletListPayload(),
        )
        .runtime,
    1 => runtime
        .dispatch(
          command: SovereignMarkdownBlockCommands.toggleOrderedList,
          payload: const SovereignToggleOrderedListPayload(),
        )
        .runtime,
    2 => runtime
        .dispatch(
          command: SovereignMarkdownBlockCommands.toggleQuote,
          payload: const SovereignToggleQuotePayload(),
        )
        .runtime,
    _ => runtime
        .dispatch(
          command: SovereignMarkdownBlockCommands.toggleTaskList,
          payload: const SovereignToggleTaskListPayload(),
        )
        .runtime,
  };
}

void _expectRuntimeInvariants(SovereignEditorRuntime runtime) {
  final state = runtime.state;
  final length = state.markdown.length;
  expect(state.document.length, length);
  expect(state.selection.baseOffset, inInclusiveRange(0, length));
  expect(state.selection.extentOffset, inInclusiveRange(0, length));
  expect(state.selection.start, inInclusiveRange(0, length));
  expect(state.selection.end, inInclusiveRange(0, length));
  expect(state.document.markdown, state.markdown);
}

void _nextControllerState(
  SovereignFlutterController controller,
  Random random,
) {
  final action = random.nextInt(16);
  switch (action) {
    case 0:
    case 1:
    case 2:
    case 3:
    case 4:
      _controllerInsertRandomText(controller, random);
      return;
    case 5:
      controller.dispatch(
        command: SovereignMarkdownInputCommands.handleEnter,
        payload: const SovereignHandleEnterPayload(),
      );
      return;
    case 6:
      controller.dispatch(
        command: SovereignMarkdownInputCommands.handleBackspace,
        payload: const SovereignHandleBackspacePayload(),
      );
      return;
    case 7:
      _controllerApplyRandomSelection(controller, random);
      return;
    case 8:
      if (controller.runtime.canUndo) controller.undo();
      return;
    case 9:
      if (controller.runtime.canRedo) controller.redo();
      return;
    case 10:
      _controllerReplaceRandomRange(controller, random);
      return;
    case 11:
      controller.dispatch(
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.strong,
        ),
      );
      return;
    case 12:
      controller.dispatch(
        command: SovereignMarkdownInlineCommands.toggleInlineStyle,
        payload: const SovereignToggleInlineStylePayload(
          SovereignMarkdownInlineStyle.inlineCode,
        ),
      );
      return;
    default:
      _controllerToggleBlockShape(controller, random);
  }
}

void _controllerInsertRandomText(
  SovereignFlutterController controller,
  Random random,
) {
  const samples = [
    'a',
    ' ',
    '>',
    '-',
    '*',
    '1',
    '.',
    '[',
    ']',
    '(',
    ')',
    '`',
    '~',
    '|',
    '&amp;',
    '\n',
    'foo',
    '```',
    '- [ ] ',
    '| A | B |\n| --- | --- |\n',
  ];
  controller.dispatch(
    command: SovereignCoreEditingCommands.insertText,
    payload: SovereignInsertTextPayload(
      samples[random.nextInt(samples.length)],
    ),
  );
}

void _controllerApplyRandomSelection(
  SovereignFlutterController controller,
  Random random,
) {
  final length = controller.markdown.length;
  final base = random.nextInt(length + 1);
  final extent = random.nextInt(length + 1);
  controller.applyTransaction(
    SovereignTransaction(
      operations: const [],
      selectionAfter: SovereignSelection(
        baseOffset: base,
        extentOffset: extent,
      ),
      metadata: const SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.selection,
        userEvent: 'test.parserFuzz.selection',
        addToHistory: false,
      ),
    ),
  );
}

void _controllerReplaceRandomRange(
  SovereignFlutterController controller,
  Random random,
) {
  final length = controller.markdown.length;
  final a = random.nextInt(length + 1);
  final b = random.nextInt(length + 1);
  final start = min(a, b);
  final end = max(a, b);
  const replacements = [
    '',
    '\n',
    'text',
    '> ',
    '- ',
    '1. ',
    '```dart\n',
    '[label](https://example.com)',
    '&lt;',
  ];
  final replacement = replacements[random.nextInt(replacements.length)];
  controller.applyTransaction(
    SovereignTransaction.single(
      SovereignSourceOperation.replace(
        replacedRange: SovereignSourceRange(start, end),
        replacementText: replacement,
      ),
      selectionAfter: SovereignSelection.collapsed(
        start + replacement.length,
      ),
      metadata: SovereignTransactionMetadata(
        intent: SovereignTransactionIntent.input,
        userEvent: 'test.parserFuzz.replace',
        parseInvalidationRange: SovereignSourceRange(start, end),
        projectionInvalidationRange: SovereignSourceRange(start, end),
      ),
    ),
  );
}

void _controllerToggleBlockShape(
  SovereignFlutterController controller,
  Random random,
) {
  switch (random.nextInt(5)) {
    case 0:
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.toggleBulletList,
        payload: const SovereignToggleBulletListPayload(),
      );
      return;
    case 1:
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.toggleOrderedList,
        payload: const SovereignToggleOrderedListPayload(),
      );
      return;
    case 2:
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.toggleQuote,
        payload: const SovereignToggleQuotePayload(),
      );
      return;
    case 3:
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.toggleTaskList,
        payload: const SovereignToggleTaskListPayload(),
      );
      return;
    default:
      controller.dispatch(
        command: SovereignMarkdownBlockCommands.insertThematicBreak,
        payload: const SovereignInsertThematicBreakPayload(),
      );
  }
}

Future<void> _expectParserBackedControllerInvariants(
  SovereignFlutterController controller,
  SovereignMarkdownParseBackend backend,
) async {
  _expectRuntimeInvariants(controller.runtime);
  final result = await backend.parse(
    SovereignMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: SovereignMarkdownProfile.commonMarkGfm,
    ),
  );
  _expectNoParseErrors(result);
  _expectParseRangesValid(result, controller.markdown.length);

  final projection = SovereignProjection.fromParseResult(result);
  projection.projectText(controller.markdown);
  SovereignRenderPlan.fromParseResult(
    parseResult: result,
    projection: projection,
  );

  expect(controller.applyParseResult(result), isTrue);
  expect(controller.hasAuthoritativeRenderPlan, isTrue);
}

void _expectNoParseErrors(SovereignMarkdownParseResult result) {
  expect(
    result.diagnostics.where(
      (diagnostic) => diagnostic.extensions['isError'] == true,
    ),
    isEmpty,
  );
}

void _expectParseRangesValid(
  SovereignMarkdownParseResult result,
  int sourceLength,
) {
  bool validRange(SovereignSourceRange range) {
    return range.start >= 0 &&
        range.end <= sourceLength &&
        range.start < range.end;
  }

  for (final block in _allBlocks(result.blocks)) {
    expect(validRange(block.sourceRange), isTrue,
        reason: 'invalid block ${block.type} ${block.sourceRange}');
  }
  for (final token in result.inlineTokens) {
    expect(validRange(token.sourceRange), isTrue,
        reason: 'invalid inline ${token.type} ${token.sourceRange}');
  }
  for (final range in result.hiddenRanges) {
    expect(validRange(range.sourceRange), isTrue,
        reason: 'invalid hidden ${range.type} ${range.sourceRange}');
  }
  for (final range in result.replacementRanges) {
    expect(validRange(range.sourceRange), isTrue,
        reason: 'invalid replacement ${range.type} ${range.sourceRange}');
    expect(range.replacementText, isNotEmpty,
        reason: 'empty replacement ${range.type} ${range.sourceRange}');
  }
  for (final zone in result.ambiguityZones) {
    expect(validRange(zone.sourceRange), isTrue,
        reason: 'invalid ambiguity ${zone.type} ${zone.sourceRange}');
  }
}

Iterable<SovereignMarkdownBlockNode> _allBlocks(
  Iterable<SovereignMarkdownBlockNode> blocks,
) sync* {
  for (final block in blocks) {
    yield block;
    yield* _allBlocks(block.children);
  }
}
