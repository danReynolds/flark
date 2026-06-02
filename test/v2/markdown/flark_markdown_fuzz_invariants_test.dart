import 'dart:math';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/flark_test_paths.dart';

void main() {
  group('Flark markdown editing fuzz invariants', () {
    for (final seed in [11, 29, 47, 83]) {
      test(
        'mixed editing session stays internally consistent for seed $seed',
        () {
          var runtime = FlarkEditorRuntime.fromMarkdown(
            '',
            extensions: FlarkMarkdownEditingExtensions.standard(),
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
        },
      );
    }
  });

  group('Flark parser-backed markdown editing fuzz invariants', () {
    final libPath = flarkNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; parser-backed fuzz suite skipped', () {
        debugPrint(
          'Skipped parser-backed markdown fuzz: native bridge missing.',
        );
        expect(true, isTrue);
      });
      return;
    }

    final backend = FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    for (final seed in [101, 202, 303]) {
      test(
        'mixed live-edit parse adoption stays consistent for seed $seed',
        () async {
          final controller = FlarkFlutterController.fromMarkdown(
            '',
            extensions: FlarkMarkdownEditingExtensions.standard(),
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
        },
      );
    }
  });
}

FlarkEditorRuntime _nextRuntime(FlarkEditorRuntime runtime, Random random) {
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

FlarkEditorRuntime _insertRandomText(
  FlarkEditorRuntime runtime,
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
        command: FlarkCoreEditingCommands.insertText,
        payload: FlarkInsertTextPayload(
          samples[random.nextInt(samples.length)],
        ),
      )
      .runtime;
}

FlarkEditorRuntime _dispatchEnter(FlarkEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: FlarkMarkdownInputCommands.handleEnter,
        payload: const FlarkHandleEnterPayload(),
      )
      .runtime;
}

FlarkEditorRuntime _dispatchBackspace(FlarkEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: FlarkMarkdownInputCommands.handleBackspace,
        payload: const FlarkHandleBackspacePayload(),
      )
      .runtime;
}

FlarkEditorRuntime _applyRandomSelection(
  FlarkEditorRuntime runtime,
  Random random,
) {
  final length = runtime.state.markdown.length;
  final base = random.nextInt(length + 1);
  final extent = random.nextInt(length + 1);
  return runtime
      .applyTransaction(
        FlarkTransaction(
          operations: const [],
          selectionAfter: FlarkSelection(
            baseOffset: base,
            extentOffset: extent,
          ),
          metadata: const FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.selection,
            userEvent: 'test.fuzz.selection',
            addToHistory: false,
          ),
        ),
      )
      .runtime;
}

FlarkEditorRuntime _replaceRandomRange(
  FlarkEditorRuntime runtime,
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
        FlarkTransaction.single(
          FlarkSourceOperation.replace(
            replacedRange: FlarkSourceRange(start, end),
            replacementText: replacement,
          ),
          selectionAfter: FlarkSelection.collapsed(start + replacement.length),
          metadata: FlarkTransactionMetadata(
            intent: FlarkTransactionIntent.input,
            userEvent: 'test.fuzz.replace',
            parseInvalidationRange: FlarkSourceRange(start, end),
            projectionInvalidationRange: FlarkSourceRange(start, end),
          ),
        ),
      )
      .runtime;
}

FlarkEditorRuntime _toggleInlineStrong(FlarkEditorRuntime runtime) {
  return runtime
      .dispatch(
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      )
      .runtime;
}

FlarkEditorRuntime _toggleBlockShape(
  FlarkEditorRuntime runtime,
  Random random,
) {
  final command = random.nextInt(4);
  return switch (command) {
    0 =>
      runtime
          .dispatch(
            command: FlarkMarkdownBlockCommands.toggleBulletList,
            payload: const FlarkToggleBulletListPayload(),
          )
          .runtime,
    1 =>
      runtime
          .dispatch(
            command: FlarkMarkdownBlockCommands.toggleOrderedList,
            payload: const FlarkToggleOrderedListPayload(),
          )
          .runtime,
    2 =>
      runtime
          .dispatch(
            command: FlarkMarkdownBlockCommands.toggleQuote,
            payload: const FlarkToggleQuotePayload(),
          )
          .runtime,
    _ =>
      runtime
          .dispatch(
            command: FlarkMarkdownBlockCommands.toggleTaskList,
            payload: const FlarkToggleTaskListPayload(),
          )
          .runtime,
  };
}

void _expectRuntimeInvariants(FlarkEditorRuntime runtime) {
  final state = runtime.state;
  final length = state.markdown.length;
  expect(state.document.length, length);
  expect(state.selection.baseOffset, inInclusiveRange(0, length));
  expect(state.selection.extentOffset, inInclusiveRange(0, length));
  expect(state.selection.start, inInclusiveRange(0, length));
  expect(state.selection.end, inInclusiveRange(0, length));
  expect(state.document.markdown, state.markdown);
}

void _nextControllerState(FlarkFlutterController controller, Random random) {
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
        command: FlarkMarkdownInputCommands.handleEnter,
        payload: const FlarkHandleEnterPayload(),
      );
      return;
    case 6:
      controller.dispatch(
        command: FlarkMarkdownInputCommands.handleBackspace,
        payload: const FlarkHandleBackspacePayload(),
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
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.strong,
        ),
      );
      return;
    case 12:
      controller.dispatch(
        command: FlarkMarkdownInlineCommands.toggleInlineStyle,
        payload: const FlarkToggleInlineStylePayload(
          FlarkMarkdownInlineStyle.inlineCode,
        ),
      );
      return;
    default:
      _controllerToggleBlockShape(controller, random);
  }
}

void _controllerInsertRandomText(
  FlarkFlutterController controller,
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
    command: FlarkCoreEditingCommands.insertText,
    payload: FlarkInsertTextPayload(samples[random.nextInt(samples.length)]),
  );
}

void _controllerApplyRandomSelection(
  FlarkFlutterController controller,
  Random random,
) {
  final length = controller.markdown.length;
  final base = random.nextInt(length + 1);
  final extent = random.nextInt(length + 1);
  controller.applyTransaction(
    FlarkTransaction(
      operations: const [],
      selectionAfter: FlarkSelection(baseOffset: base, extentOffset: extent),
      metadata: const FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.selection,
        userEvent: 'test.parserFuzz.selection',
        addToHistory: false,
      ),
    ),
  );
}

void _controllerReplaceRandomRange(
  FlarkFlutterController controller,
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
    FlarkTransaction.single(
      FlarkSourceOperation.replace(
        replacedRange: FlarkSourceRange(start, end),
        replacementText: replacement,
      ),
      selectionAfter: FlarkSelection.collapsed(start + replacement.length),
      metadata: FlarkTransactionMetadata(
        intent: FlarkTransactionIntent.input,
        userEvent: 'test.parserFuzz.replace',
        parseInvalidationRange: FlarkSourceRange(start, end),
        projectionInvalidationRange: FlarkSourceRange(start, end),
      ),
    ),
  );
}

void _controllerToggleBlockShape(
  FlarkFlutterController controller,
  Random random,
) {
  switch (random.nextInt(5)) {
    case 0:
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.toggleBulletList,
        payload: const FlarkToggleBulletListPayload(),
      );
      return;
    case 1:
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.toggleOrderedList,
        payload: const FlarkToggleOrderedListPayload(),
      );
      return;
    case 2:
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.toggleQuote,
        payload: const FlarkToggleQuotePayload(),
      );
      return;
    case 3:
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.toggleTaskList,
        payload: const FlarkToggleTaskListPayload(),
      );
      return;
    default:
      controller.dispatch(
        command: FlarkMarkdownBlockCommands.insertThematicBreak,
        payload: const FlarkInsertThematicBreakPayload(),
      );
  }
}

Future<void> _expectParserBackedControllerInvariants(
  FlarkFlutterController controller,
  FlarkMarkdownParseBackend backend,
) async {
  _expectRuntimeInvariants(controller.runtime);
  final result = await backend.parse(
    FlarkMarkdownParseRequest(
      revision: controller.state.revision,
      markdown: controller.markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
  _expectNoParseErrors(result);
  _expectParseRangesValid(result, controller.markdown.length);

  final projection = FlarkProjection.fromParseResult(result);
  projection.projectText(controller.markdown);
  FlarkRenderPlan.fromParseResult(parseResult: result, projection: projection);

  expect(controller.applyParseResult(result), isTrue);
  expect(controller.hasAuthoritativeRenderPlan, isTrue);
}

void _expectNoParseErrors(FlarkMarkdownParseResult result) {
  expect(
    result.diagnostics.where(
      (diagnostic) => diagnostic.extensions['isError'] == true,
    ),
    isEmpty,
  );
}

void _expectParseRangesValid(
  FlarkMarkdownParseResult result,
  int sourceLength,
) {
  bool validRange(FlarkSourceRange range) {
    return range.start >= 0 &&
        range.end <= sourceLength &&
        range.start < range.end;
  }

  for (final block in _allBlocks(result.blocks)) {
    expect(
      validRange(block.sourceRange),
      isTrue,
      reason: 'invalid block ${block.type} ${block.sourceRange}',
    );
  }
  for (final token in result.inlineTokens) {
    expect(
      validRange(token.sourceRange),
      isTrue,
      reason: 'invalid inline ${token.type} ${token.sourceRange}',
    );
  }
  for (final range in result.hiddenRanges) {
    expect(
      validRange(range.sourceRange),
      isTrue,
      reason: 'invalid hidden ${range.type} ${range.sourceRange}',
    );
  }
  for (final range in result.replacementRanges) {
    expect(
      validRange(range.sourceRange),
      isTrue,
      reason: 'invalid replacement ${range.type} ${range.sourceRange}',
    );
    expect(
      range.replacementText,
      isNotEmpty,
      reason: 'empty replacement ${range.type} ${range.sourceRange}',
    );
  }
  for (final zone in result.ambiguityZones) {
    expect(
      validRange(zone.sourceRange),
      isTrue,
      reason: 'invalid ambiguity ${zone.type} ${zone.sourceRange}',
    );
  }
}

Iterable<FlarkMarkdownBlockNode> _allBlocks(
  Iterable<FlarkMarkdownBlockNode> blocks,
) sync* {
  for (final block in blocks) {
    yield block;
    yield* _allBlocks(block.children);
  }
}
