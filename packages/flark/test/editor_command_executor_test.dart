import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  group(
    'editor command executor',
    () {
      late FlarkCoreDocument document;
      late FlarkCoreEditorSession session;
      late FlarkEditorCoordinator coordinator;
      late FlarkEditorCommandExecutor commands;

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
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
      }

      FlarkCoreSelectionSnapshot caret(int offset) =>
          FlarkCoreSelectionSnapshot(base: offset, extent: offset);

      FlarkSourceEditCommand insert(int offset, String text) =>
          FlarkSourceEditCommand(
            startUtf16: offset,
            endUtf16: offset,
            replacement: text,
            beforeSelection: caret(offset),
            afterSelection: caret(offset + text.length),
            coalesceTyping: true,
          );

      test(
        'owns command admission, source publication, and settlement',
        () async {
          await open('base\n');

          final execution = commands.executeSourceEdit(insert(0, 'x'));

          expect(execution.generation, 1);
          expect(coordinator.editGeneration, 1);
          expect(coordinator.publishedSourceGeneration, 1);
          expect(coordinator.pendingEdits, 1);
          await execution.result;
          expect(await document.readSource(), 'xbase\n');

          commands.complete(execution);
          expect(coordinator.pendingEdits, 0);
          expect(() => commands.complete(execution), throwsStateError);
        },
      );

      test('serializes history after admitted source edits', () async {
        await open('base\n');

        final edit = commands.executeSourceEdit(insert(0, 'x'));
        final history = commands.executeHistory(FlarkHistoryDirection.undo);

        expect(coordinator.pendingEdits, 2);
        await edit.result;
        commands.complete(edit);
        final outcome = await history.result;
        commands.complete(history);

        expect(outcome, isA<FlarkCoreHistoryReplayed>());
        expect(await document.readSource(), 'base\n');
        expect(coordinator.pendingEdits, 0);
      });

      test(
        'rejects command admission once the coordinator is closing',
        () async {
          await open('base\n');
          coordinator.beginClosing();

          expect(
            () => commands.executeSourceEdit(insert(0, 'x')),
            throwsStateError,
          );
        },
      );
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to run native editor tests.'
        : false,
  );
}
