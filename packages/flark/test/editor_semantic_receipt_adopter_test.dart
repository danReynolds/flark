import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  group(
    'editor semantic receipt adopter',
    () {
      late FlarkCoreDocument document;
      late FlarkCoreEditorSession session;
      late FlarkEditorCoordinator coordinator;
      late FlarkEditorCommandExecutor commands;
      late FlarkEditorViewportState viewportState;
      late FlarkEditorSemanticReceiptAdopter adopter;
      late int activeOrdinal;

      Future<void> open(String source) async {
        document = await FlarkCoreDocument.open(
          source,
          libraryPath: libraryPath!,
        );
        session = FlarkCoreEditorSession(document);
        coordinator = FlarkEditorCoordinator();
        commands = FlarkEditorCommandExecutor(
          coordinator: coordinator,
          session: session,
        );
        viewportState = FlarkEditorViewportState();
        final pager = FlarkEditorViewportPager(
          source: document,
          coordinator: coordinator,
          maximumVisibleBytes: 16 * 1024,
          rowsPerPage: 32,
          maximumCaretPageHops: 513,
        );
        adopter = FlarkEditorSemanticReceiptAdopter(
          coordinator: coordinator,
          commands: commands,
          viewportState: viewportState,
          viewportPager: pager,
          maximumVisibleCodeUnits: 16 * 1024,
        );
        await document.pumpUntilReady();
        final viewport = await document.queryViewport(maxRows: 32);
        final visibleSource = await document.readSourceRange(
          viewport.coveredBytes.start,
          viewport.coveredBytes.end,
        );
        viewportState.install(viewport, visibleSource);
        activeOrdinal = viewport.rows.single.ordinal;
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
      }

      FlarkEditorSemanticReceiptAdoptionRequest request({
        required FlarkEditorCommandExecution<FlarkCoreEditIntentOutcomeV1>
        execution,
        required FlarkCoreEditIntentOutcomeV1 outcome,
      }) => FlarkEditorSemanticReceiptAdoptionRequest(
        execution: execution,
        outcome: outcome,
        inputGlobalUtf16Start: 0,
        inputValue: const FlarkEditorInputValue(
          text: 'abc',
          selection: FlarkTextSelection.collapsed(offset: 3),
        ),
        activeOrdinal: activeOrdinal,
        selectionBaseUtf16: 3,
        selectionExtentUtf16: 3,
        crossRowSelection: false,
      );

      test(
        'adopts one current semantic receipt across portable state',
        () async {
          await open('abc');
          await session.setSelectionUtf16(3, 3);
          final execution = commands.executeSemanticEdit(
            FlarkCoreEditIntentV1.insertParagraphBreak,
          );
          final outcome = await execution.result;

          final adoption = adopter.adopt(
            request(execution: execution, outcome: outcome),
          );

          expect(adoption, isNotNull);
          expect(adoption!.caretUtf16, outcome.receipt.resultSelectionUtf16);
          expect(coordinator.publishedSourceGeneration, execution.generation);
          expect(viewportState.visibleSource, 'abc\n\n');
          expect(viewportState.semanticCurrent, isFalse);
          expect(
            coordinator.pendingPresentation.paragraphGap != null ||
                coordinator.pendingPresentation.structuralSurfaces.isNotEmpty,
            isTrue,
          );
          commands.complete(execution);
        },
      );

      test(
        'a superseded semantic receipt cannot mutate portable state',
        () async {
          await open('abc');
          await session.setSelectionUtf16(3, 3);
          final semantic = commands.executeSemanticEdit(
            FlarkCoreEditIntentV1.insertParagraphBreak,
          );
          final outcome = await semantic.result;
          final later = commands.executeSourceEdit(
            FlarkSourceEditCommand(
              startUtf16: outcome.receipt.resultSelectionUtf16,
              endUtf16: outcome.receipt.resultSelectionUtf16,
              replacement: 'x',
              beforeSelection: FlarkCoreSelectionSnapshot(
                base: outcome.receipt.resultSelectionUtf16,
                extent: outcome.receipt.resultSelectionUtf16,
              ),
              afterSelection: FlarkCoreSelectionSnapshot(
                base: outcome.receipt.resultSelectionUtf16 + 1,
                extent: outcome.receipt.resultSelectionUtf16 + 1,
              ),
              coalesceTyping: true,
            ),
          );

          final adoption = adopter.adopt(
            request(execution: semantic, outcome: outcome),
          );

          expect(adoption, isNull);
          expect(viewportState.visibleSource, 'abc');
          expect(coordinator.pendingPresentation.isEmpty, isTrue);
          await later.result;
          commands.complete(semantic);
          commands.complete(later);
        },
      );
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to run native editor tests.'
        : false,
  );
}
