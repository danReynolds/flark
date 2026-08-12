import 'dart:io';

import 'package:flark_core/flark_core.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  group('grapheme policy', () {
    test('deletes one extended cluster including emoji ZWJ sequences', () {
      const family = '\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}';
      final text = 'ab$family!';
      final beforeBang = text.length - 1;
      expect(FlarkCoreGraphemePolicy.previousClusterRange(text, beforeBang), (
        2,
        beforeBang,
      ));
      expect(FlarkCoreGraphemePolicy.previousClusterRange(text, 2), (1, 2));
      expect(FlarkCoreGraphemePolicy.previousClusterRange(text, 0), isNull);
      expect(FlarkCoreGraphemePolicy.nextClusterRange(text, 2), (
        2,
        beforeBang,
      ));
      expect(
        FlarkCoreGraphemePolicy.nextClusterRange(text, text.length),
        isNull,
      );
      expect(FlarkCoreGraphemePolicy.isSingleCluster(family), isTrue);
      expect(FlarkCoreGraphemePolicy.isSingleCluster('ab'), isFalse);
      expect(FlarkCoreGraphemePolicy.isSingleCluster(''), isFalse);

      final oversizedCluster = 'a${List.filled(3000, '\u0301').join()}';
      expect(
        FlarkCoreGraphemePolicy.clusterBoundaryAtOrBefore(
          oversizedCluster,
          2048,
        ),
        0,
      );
      expect(
        FlarkCoreGraphemePolicy.clusterBoundaryAtOrAfter(
          oversizedCluster,
          2048,
        ),
        oversizedCluster.length,
      );
    });
  });

  group(
    'editor session',
    () {
      late FlarkCoreDocument document;
      late int clockMicros;
      late FlarkCoreEditorSession session;

      Future<void> open(
        String source, {
        Duration editIntentReplyTimeout = const Duration(milliseconds: 250),
        bool debugDropFirstEditIntentReply = false,
      }) async {
        document = await FlarkCoreDocument.open(
          source,
          libraryPath: libraryPath!,
          editIntentReplyTimeout: editIntentReplyTimeout,
          debugDropFirstEditIntentReply: debugDropFirstEditIntentReply,
        );
        clockMicros = 0;
        session = FlarkCoreEditorSession(
          document,
          clockMicros: () => clockMicros,
        );
      }

      FlarkCoreSelectionSnapshot caret(int offset) =>
          FlarkCoreSelectionSnapshot(base: offset, extent: offset);

      Future<FlarkCoreEditReceipt> type(int offset, String cluster) =>
          session.applyEditUtf16(
            offset,
            offset,
            cluster,
            beforeSelection: caret(offset),
            afterSelection: caret(offset + cluster.length),
            coalesceTyping: true,
          );

      test(
        'rapid typing coalesces into one unit and undoes atomically',
        () async {
          await open('base\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });

          await type(0, 'a');
          clockMicros += 400000;
          await type(1, 'b');
          clockMicros += 400000;
          await type(2, 'c');
          expect(await document.readSource(), 'abcbase\n');
          expect((await session.resolveSelection())!.extent, 3);

          final outcome = await session.undo();
          expect(outcome, isA<FlarkCoreHistoryReplayed>());
          expect(outcome!.restoreSelection.extent, 0);
          expect(await document.readSource(), 'base\n');
          expect((await session.resolveSelection())!.extent, 0);
          expect(session.canUndo, isFalse);
          expect(session.canRedo, isTrue);

          final redone = await session.redo();
          expect(redone, isA<FlarkCoreHistoryReplayed>());
          expect(redone!.restoreSelection.extent, 3);
          expect(await document.readSource(), 'abcbase\n');
          expect((await session.resolveSelection())!.extent, 3);
        },
      );

      test(
        'idle gaps, epochs, and non-typing edits break coalescing',
        () async {
          await open('base\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });

          await type(0, 'a');
          clockMicros += 1000001;
          await type(1, 'b');
          expect(await document.readSource(), 'abbase\n');
          await session.undo();
          expect(await document.readSource(), 'abase\n');
          await session.undo();
          expect(await document.readSource(), 'base\n');

          await type(0, 'x');
          session.breakTypingGroup();
          clockMicros += 1;
          await type(1, 'y');
          await session.undo();
          expect(await document.readSource(), 'xbase\n');

          // A multi-cluster replacement never joins a typing run.
          await session.applyEditUtf16(
            0,
            0,
            'multi',
            beforeSelection: caret(0),
            afterSelection: caret(5),
          );
          await session.undo();
          expect(await document.readSource(), 'xbase\n');
        },
      );

      test(
        'composition updates group into one unit and commit joins it',
        () async {
          await open('base\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });

          await session.applyEditUtf16(
            0,
            0,
            'k',
            beforeSelection: caret(0),
            afterSelection: caret(1),
            compositionGroup: session.compositionGroupForMutation(
              composingActive: true,
            ),
          );
          await session.applyEditUtf16(
            0,
            1,
            'ka',
            beforeSelection: caret(1),
            afterSelection: caret(2),
            compositionGroup: session.compositionGroupForMutation(
              composingActive: true,
            ),
          );
          // The commit mutation ends composition but still joins the group.
          await session.applyEditUtf16(
            0,
            2,
            'か',
            beforeSelection: caret(2),
            afterSelection: caret(1),
            compositionGroup: session.compositionGroupForMutation(
              composingActive: false,
            ),
          );
          expect(await document.readSource(), 'かbase\n');

          final outcome = await session.undo();
          expect(outcome, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
          expect(session.canUndo, isFalse);
        },
      );

      test('tracking a composition without mutation reports its end', () async {
        await open('base\n');
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        expect(
          session.trackCompositionWithoutMutation(composingActive: true),
          isFalse,
        );
        expect(
          session.trackCompositionWithoutMutation(composingActive: false),
          isTrue,
        );
        expect(
          session.trackCompositionWithoutMutation(composingActive: false),
          isFalse,
        );
      });

      test(
        'semantic Return commits from canonical anchors and remains one undo unit',
        () async {
          await open('- one\n- two\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(5, 5);

          final receipt = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(receipt.disposition, FlarkCoreEditIntentDispositionV1.applied);
          expect(
            receipt.presentationTransition,
            FlarkCoreEditPresentationTransitionV1.continueList,
          );
          expect(receipt.replacement, '\n- ');
          expect(receipt.resultSelectionUtf16, 8);
          expect(await document.readSource(), '- one\n- \n- two\n');
          expect((await session.resolveSelection())!.extent, 8);

          final undone = await session.undo();
          expect(undone, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), '- one\n- two\n');
          expect((await session.resolveSelection())!.extent, 5);

          final next = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(next.disposition, FlarkCoreEditIntentDispositionV1.applied);
          expect(await document.readSource(), '- one\n- \n- two\n');

          final nextUndone = await session.undo();
          expect(nextUndone, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), '- one\n- two\n');

          final redone = await session.redo();
          expect(redone, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), '- one\n- \n- two\n');
          expect((await session.resolveSelection())!.extent, 8);
        },
      );

      test('lost semantic reply recovers the terminal exactly once', () async {
        await open(
          '- one\n- two\n',
          editIntentReplyTimeout: const Duration(milliseconds: 10),
          debugDropFirstEditIntentReply: true,
        );
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        await session.setSelectionUtf16(5, 5);
        final revision = document.revision;

        final receipt = await session.applyEditIntentV1(
          FlarkCoreEditIntentV1.insertParagraphBreak,
          compositionActive: false,
        );

        expect(receipt.disposition, FlarkCoreEditIntentDispositionV1.applied);
        expect(document.revision, revision + 1);
        expect(await document.readSource(), '- one\n- \n- two\n');
        expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
        expect(await document.readSource(), '- one\n- two\n');
        expect(session.canUndo, isFalse);
      });

      test(
        'a noncommitting semantic terminal is acknowledged in order',
        () async {
          await open('- one\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(5, 5);

          final guarded = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: true,
          );
          expect(
            guarded.disposition,
            FlarkCoreEditIntentDispositionV1.notApplicable,
          );

          final applied = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(applied.disposition, FlarkCoreEditIntentDispositionV1.applied);
          expect(await document.readSource(), '- one\n- \n');
        },
      );

      test('canonical selection anchors survive edits by affinity', () async {
        await open('Hello world!\n');
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });

        // Range selecting "world": start 6, end 11, backwards (base 11).
        final generation = await session.setSelectionUtf16(
          11,
          6,
          adapterState: 'shadow',
        );
        expect(generation, session.selectionGeneration);

        // Insertion before the range shifts both endpoints.
        await document.applyEditUtf16(0, 0, '>> ');
        var resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (14, 9));
        expect(resolved.adapterState, 'shadow');
        expect(resolved.revision, document.revision);

        // Insertion exactly at the range start stays outside the range.
        await document.applyEditUtf16(9, 9, '!');
        resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (15, 10));

        // Insertion exactly at the range end stays outside the range.
        await document.applyEditUtf16(15, 15, '?');
        resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (15, 10));

        // A collapsed caret follows text typed at it.
        await session.setSelectionUtf16(10, 10);
        await document.applyEditUtf16(10, 10, 'zz');
        resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (12, 12));
        expect(resolved.isCollapsed, isTrue);

        // Undo of a document-level edit transforms the anchors back.
        final receipt = await document.applyEditUtf16(0, 3, '');
        resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (9, 9));
        await document.replayHistory(receipt.historyToken!);
        resolved = await session.resolveSelection();
        expect((resolved!.base, resolved.extent), (12, 12));

        await session.clearSelection();
        expect(await session.resolveSelection(), isNull);
        final inspection = await document.inspectSession();
        expect(inspection.liveAnchors, 0);
      });

      test('a disabled history budget clears undo instead of lying', () async {
        document = await FlarkCoreDocument.open(
          'base\n',
          libraryPath: libraryPath!,
          historyBudgetBytes: 0,
        );
        session = FlarkCoreEditorSession(document, clockMicros: () => 0);
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        final receipt = await type(0, 'a');
        expect(
          receipt.historyDisposition,
          FlarkCoreHistoryDisposition.disabled,
        );
        expect(session.canUndo, isFalse);
        expect(await session.undo(), isNull);
      });

      test(
        'independent editor sessions serialize without cross-talk',
        () async {
          final firstDocument = await FlarkCoreDocument.open(
            '- one\n',
            libraryPath: libraryPath!,
          );
          final secondDocument = await FlarkCoreDocument.open(
            '9) nine\n',
            libraryPath: libraryPath,
          );
          final firstSession = FlarkCoreEditorSession(firstDocument);
          final secondSession = FlarkCoreEditorSession(secondDocument);
          addTearDown(() async {
            await firstSession.dispose();
            await secondSession.dispose();
            await firstDocument.dispose();
            await secondDocument.dispose();
          });

          await Future.wait([
            firstSession.setSelectionUtf16(5, 5),
            secondSession.setSelectionUtf16(7, 7),
          ]);
          final receipts = await Future.wait([
            firstSession.applyEditIntentV1(
              FlarkCoreEditIntentV1.insertParagraphBreak,
              compositionActive: false,
            ),
            secondSession.applyEditIntentV1(
              FlarkCoreEditIntentV1.insertParagraphBreak,
              compositionActive: false,
            ),
          ]);

          expect(receipts.every((receipt) => receipt.hasCommit), isTrue);
          expect(await firstDocument.readSource(), '- one\n- \n');
          expect(await secondDocument.readSource(), '9) nine\n10) \n');
          expect(firstDocument.revision, 2);
          expect(secondDocument.revision, 2);
        },
      );

      test('semantic edits accept a collapsed upstream visual caret', () async {
        await open('- one\n');
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });

        await session.setSelectionUtf16(
          5,
          5,
          affinity: FlarkCoreAffinity.upstream,
          adapterState: 'upstream-caret',
        );
        final receipt = await session.applyEditIntentV1(
          FlarkCoreEditIntentV1.insertParagraphBreak,
          compositionActive: false,
        );

        expect(receipt.disposition, FlarkCoreEditIntentDispositionV1.applied);
        expect(await document.readSource(), '- one\n- \n');
        final selection = await session.resolveSelection();
        expect(selection!.affinity, FlarkCoreAffinity.upstream);
        expect(selection.adapterState, 'upstream-caret');
        expect(selection.extent, 8);
      });

      test('worker loss fail-stops the editor session', () async {
        await open('- one\n');
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        await session.setSelectionUtf16(5, 5);

        final crash = document.debugCrashWorkerForTesting();
        final edit = session.applyEditIntentV1(
          FlarkCoreEditIntentV1.insertParagraphBreak,
          compositionActive: false,
        );

        await expectLater(crash, throwsA(isA<FlarkCoreWorkerException>()));
        await expectLater(edit, throwsA(isA<FlarkCoreWorkerException>()));
        expect(session.postCommitUnknown, isTrue);
        await expectLater(
          document.readSource(),
          throwsA(isA<FlarkCoreWorkerException>()),
        );
      });
    },
    skip: libraryPath == null ? 'Set FLARK_V4_LIBRARY_PATH.' : false,
  );
}
