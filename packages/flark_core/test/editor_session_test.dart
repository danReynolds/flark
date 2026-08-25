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

          final firstReceipt = await type(0, 'a');
          expect(firstReceipt.telemetry, isNotNull);
          expect(
            firstReceipt.telemetry!.coreQueueMicros,
            greaterThanOrEqualTo(0),
          );
          expect(
            firstReceipt.telemetry!.workerRoundTripMicros,
            greaterThanOrEqualTo(firstReceipt.telemetry!.nativeFfiMicros),
          );
          clockMicros += 400000;
          await type(1, 'b');
          clockMicros += 400000;
          await type(2, 'c');
          expect(await document.readSource(), 'abcbase\n');
          expect((await session.resolveSelection())!.extent, 3);
          expect((await document.inspectSession()).liveHistoryTokens, 1);

          final outcome = await session.undo();
          expect(outcome, isA<FlarkCoreHistoryReplayed>());
          final undoReceipt = (outcome! as FlarkCoreHistoryReplayed).receipt;
          expect(undoReceipt.telemetry, isNotNull);
          expect(
            undoReceipt.telemetry!.workerRoundTripMicros,
            greaterThanOrEqualTo(undoReceipt.telemetry!.nativeFfiMicros),
          );
          expect(outcome.restoreSelection.extent, 0);
          expect(await document.readSource(), 'base\n');
          expect((await session.resolveSelection())!.extent, 0);
          expect(session.canUndo, isFalse);
          expect(session.canRedo, isTrue);

          final redone = await session.redo();
          expect(redone, isA<FlarkCoreHistoryReplayed>());
          expect(
            (redone! as FlarkCoreHistoryReplayed).receipt.telemetry,
            isNotNull,
          );
          expect(redone.restoreSelection.extent, 3);
          expect(await document.readSource(), 'abcbase\n');
          expect((await session.resolveSelection())!.extent, 3);
        },
      );

      test(
        'typing starts a new atomic unit at the native composite cap',
        () async {
          await open('base\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });

          for (var index = 0; index < 257; index += 1) {
            await type(index, 'x');
          }
          expect((await document.inspectSession()).liveHistoryTokens, 2);

          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect((await document.readSource()).length, 256 + 'base\n'.length);
          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
        },
      );

      test('oversized replacement retains the staged bulk lane', () async {
        await open('base\n');
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        final paste = List.filled(96 * 1024, 'p').join();

        await session.applyEditUtf16(
          0,
          0,
          paste,
          beforeSelection: caret(0),
          afterSelection: caret(paste.length),
        );
        expect(document.sourceUtf16Length, paste.length + 5);
        expect((await session.resolveSelection())!.extent, paste.length);

        expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
        expect(await document.readSource(), 'base\n');

        await session.applyEditUtf16(
          0,
          0,
          paste,
          beforeSelection: caret(0),
          afterSelection: caret(paste.length),
        );
        const deleted = 20 * 1024;
        await session.applyEditUtf16(
          0,
          deleted,
          '',
          beforeSelection: const FlarkCoreSelectionSnapshot(
            base: 0,
            extent: deleted,
          ),
          afterSelection: caret(0),
        );
        expect(document.sourceUtf16Length, paste.length + 5 - deleted);
        expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
        expect(document.sourceUtf16Length, paste.length + 5);
      });

      test(
        'staged deletion rejects before mutation without history room',
        () async {
          final source = List.filled(20 * 1024, 'x').join();
          document = await FlarkCoreDocument.open(
            source,
            libraryPath: libraryPath!,
            historyBudgetBytes: 1024,
          );
          session = FlarkCoreEditorSession(document, clockMicros: () => 0);
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          final before = FlarkCoreSelectionSnapshot(
            base: 0,
            extent: source.length,
          );
          await session.setSelectionUtf16(before.base, before.extent);
          final revision = document.revision;

          await expectLater(
            session.applyEditUtf16(
              0,
              source.length,
              '',
              beforeSelection: before,
              afterSelection: caret(0),
            ),
            throwsA(
              isA<FlarkCoreNativeException>().having(
                (error) => error.status,
                'status',
                0x0403,
              ),
            ),
          );
          expect(document.revision, revision);
          expect(await document.readSource(), source);
          expect(session.canUndo, isFalse);
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
          expect((await document.inspectSession()).liveAnchors, 4);
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
            compositionFinal: true,
          );
          expect(await document.readSource(), 'かbase\n');
          expect((await document.inspectSession()).liveHistoryTokens, 1);
          expect((await document.inspectSession()).liveAnchors, 2);

          final outcome = await session.undo();
          expect(outcome, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
          expect(session.canUndo, isFalse);
        },
      );

      test(
        'composition cancel restores its base and preserves earlier history',
        () async {
          await open('base\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });

          await session.applyEditUtf16(
            0,
            0,
            'x',
            beforeSelection: caret(0),
            afterSelection: caret(1),
          );
          await session.applyEditUtf16(
            1,
            5,
            'k',
            beforeSelection: const FlarkCoreSelectionSnapshot(
              base: 5,
              extent: 1,
            ),
            afterSelection: caret(2),
            compositionGroup: session.compositionGroupForMutation(
              composingActive: true,
            ),
          );
          await session.applyEditUtf16(
            1,
            2,
            'ka',
            beforeSelection: caret(2),
            afterSelection: caret(3),
            compositionGroup: session.compositionGroupForMutation(
              composingActive: true,
            ),
          );
          expect(await document.readSource(), 'xka\n');
          expect((await document.inspectSession()).liveHistoryTokens, 2);
          expect((await document.inspectSession()).liveAnchors, 4);

          final cancelled = await session.cancelComposition();
          expect(cancelled, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'xbase\n');
          final restoredSelection = await session.resolveSelection();
          expect(restoredSelection?.base, 5);
          expect(restoredSelection?.extent, 1);
          expect(session.canUndo, isTrue);
          expect(session.canRedo, isFalse);
          expect((await document.inspectSession()).liveHistoryTokens, 1);
          expect((await document.inspectSession()).liveAnchors, 2);

          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
          expect(session.canUndo, isFalse);
        },
      );

      test(
        'composition cancellation remains allocation-free at the anchor cap',
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
          expect((await document.inspectSession()).liveAnchors, 4);

          final held = <FlarkCoreAnchor>[];
          FlarkCoreNativeException? limit;
          for (var index = 0; index < 5000; index += 1) {
            try {
              held.add(await document.createAnchorUtf16(1, downstream: true));
            } on FlarkCoreNativeException catch (error) {
              limit = error;
              break;
            }
          }
          expect(limit?.status, 0x0403);
          expect((await document.inspectSession()).liveAnchors, 4096);

          final cancelled = await session.cancelComposition();
          expect(cancelled, isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
          expect((await session.resolveSelection())?.extent, 0);
          expect((await document.inspectSession()).liveAnchors, 4094);

          // Fill the two slots released by the former canonical pair. A new
          // composition must now reject while reserving its base, before any
          // source or history mutation can occur.
          held.add(await document.createAnchorUtf16(0, downstream: true));
          held.add(await document.createAnchorUtf16(0, downstream: true));
          final beforeRejected = await document.inspectSession();
          expect(beforeRejected.liveAnchors, 4096);
          await expectLater(
            session.applyEditUtf16(
              0,
              0,
              'z',
              beforeSelection: caret(0),
              afterSelection: caret(1),
              compositionGroup: session.compositionGroupForMutation(
                composingActive: true,
              ),
            ),
            throwsA(
              isA<FlarkCoreNativeException>().having(
                (error) => error.status,
                'status',
                0x0403,
              ),
            ),
          );
          expect(await document.readSource(), 'base\n');
          final afterRejected = await document.inspectSession();
          expect(afterRejected.revision, beforeRejected.revision);
          expect(afterRejected.liveHistoryTokens, 0);
          session.endCompositionGroup();

          for (final anchor in held) {
            await document.releaseAnchor(anchor);
          }
          expect((await document.inspectSession()).liveAnchors, 2);
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
        'metadata-only composition commit releases its base anchors',
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
          expect((await document.inspectSession()).liveAnchors, 4);
          expect(
            session.trackCompositionWithoutMutation(composingActive: false),
            isTrue,
          );

          await session.finishComposition();
          expect((await document.inspectSession()).liveAnchors, 2);
          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'base\n');
        },
      );

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

      test(
        'terminal paragraph Returns extend one canonical gap after each parse',
        () async {
          await open('fff');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(3, 3);

          const expectedSources = <String>[
            'fff\n\n',
            'fff\n\n\n',
            'fff\n\n\n\n',
          ];
          const expectedCarets = <int>[5, 6, 7];
          for (var index = 0; index < expectedSources.length; index += 1) {
            final receipt = await session.applyEditIntentV1(
              FlarkCoreEditIntentV1.insertParagraphBreak,
              compositionActive: false,
            );
            expect(
              receipt.disposition,
              FlarkCoreEditIntentDispositionV1.applied,
            );
            expect(receipt.resultSelectionUtf16, expectedCarets[index]);
            expect(await document.readSource(), expectedSources[index]);
            expect(
              (await session.resolveSelection())!.extent,
              expectedCarets[index],
            );
            await document.pumpUntilReady();
          }
        },
      );

      test(
        'terminal paragraph gap remains semantic after one literal extension',
        () async {
          await open('fff');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(3, 3);
          final split = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(split.hasCommit, isTrue);
          await document.pumpUntilReady();

          await session.applyEditUtf16(
            5,
            5,
            '\n',
            beforeSelection: caret(5),
            afterSelection: caret(6),
          );
          await document.pumpUntilReady();
          expect(await document.readSource(), 'fff\n\n\n');
          expect((await session.resolveSelection())!.extent, 6);

          final extended = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(
            extended.disposition,
            FlarkCoreEditIntentDispositionV1.applied,
          );
          expect(extended.resultSelectionUtf16, 7);
          expect(await document.readSource(), 'fff\n\n\n\n');
        },
      );

      test(
        'a queued semantic successor uses exact pending lineage without a pump',
        () async {
          await open('9) alpha\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(8, 8);

          final inserted = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.insertParagraphBreak,
            compositionActive: false,
          );
          expect(
            inserted.disposition,
            FlarkCoreEditIntentDispositionV1.applied,
          );
          expect(document.isReady, isFalse);

          final deleted = await session.applyEditIntentV1(
            FlarkCoreEditIntentV1.deleteBackward,
            compositionActive: false,
          );
          expect(deleted.disposition, FlarkCoreEditIntentDispositionV1.applied);
          expect(await document.readSource(), '9) alpha\n\n\n');
          expect((await session.resolveSelection())!.extent, 10);
        },
      );

      test(
        'task action targets its row while preserving a directional selection',
        () async {
          const initial = '- [ ] task\n\nselection\n';
          await open(initial);
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          await session.setSelectionUtf16(
            21,
            12,
            affinity: FlarkCoreAffinity.upstream,
            adapterState: 'selection-state',
          );

          final receipt = await session.applySemanticActionV1(
            FlarkCoreSemanticActionV1.toggleTaskChecked,
            targetUtf16: 6,
          );
          expect(receipt.disposition, FlarkCoreEditIntentDispositionV1.applied);
          expect(
            receipt.presentationTransition,
            FlarkCoreEditPresentationTransitionV1.toggleTaskChecked,
          );
          expect(receipt.replacement, 'x');
          expect(await document.readSource(), '- [x] task\n\nselection\n');
          var selection = (await session.resolveSelection())!;
          expect((selection.base, selection.extent), (21, 12));
          expect(selection.affinity, FlarkCoreAffinity.upstream);
          expect(selection.adapterState, 'selection-state');

          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), initial);
          selection = (await session.resolveSelection())!;
          expect((selection.base, selection.extent), (21, 12));

          expect(await session.redo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), '- [x] task\n\nselection\n');
          selection = (await session.resolveSelection())!;
          expect((selection.base, selection.extent), (21, 12));
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

      test('lost literal reply recovers the terminal exactly once', () async {
        await open(
          'base\n',
          editIntentReplyTimeout: const Duration(milliseconds: 10),
          debugDropFirstEditIntentReply: true,
        );
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        final revision = document.revision;

        await session.applyEditUtf16(
          0,
          0,
          'a',
          beforeSelection: caret(0),
          afterSelection: caret(1),
        );

        expect(document.revision, revision + 1);
        expect(await document.readSource(), 'abase\n');
        expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
        expect(await document.readSource(), 'base\n');
        expect(session.canUndo, isFalse);
      });

      test('lost staged reply recovers the terminal exactly once', () async {
        await open(
          'base\n',
          editIntentReplyTimeout: const Duration(microseconds: 1),
          debugDropFirstEditIntentReply: true,
        );
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
        final revision = document.revision;
        final paste = List.filled(96 * 1024, 'p').join();

        await session.applyEditUtf16(
          0,
          0,
          paste,
          beforeSelection: caret(0),
          afterSelection: caret(paste.length),
        );

        expect(document.revision, revision + 1);
        expect(document.sourceUtf16Length, paste.length + 5);
        expect((await session.resolveSelection())!.extent, paste.length);
        expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
        expect(await document.readSource(), 'base\n');
        expect(session.canUndo, isFalse);
      });

      test(
        'non-collapsed replacement retargets selection atomically',
        () async {
          await open('a *bold* z\n');
          addTearDown(() async {
            await session.dispose();
            await document.dispose();
          });
          final before = FlarkCoreSelectionSnapshot(base: 8, extent: 2);
          final after = caret(7);
          await session.setSelectionUtf16(before.base, before.extent);

          await session.applyEditUtf16(
            2,
            8,
            'plain',
            beforeSelection: before,
            afterSelection: after,
          );

          expect(await document.readSource(), 'a plain z\n');
          final selected = await session.resolveSelection();
          expect(selected!.base, 7);
          expect(selected.extent, 7);
          expect(await session.undo(), isA<FlarkCoreHistoryReplayed>());
          expect(await document.readSource(), 'a *bold* z\n');
          final restored = await session.resolveSelection();
          expect(restored!.base, 8);
          expect(restored.extent, 2);
        },
      );

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

      test('required history rejects before mutation when disabled', () async {
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
        await expectLater(
          type(0, 'a'),
          throwsA(
            isA<FlarkCoreNativeException>().having(
              (error) => error.status,
              'status',
              0x0403,
            ),
          ),
        );
        expect(await document.readSource(), 'base\n');
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
