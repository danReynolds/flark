import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

import 'support/live_editor_transition_probe.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  test(
    'syntax hazards never relay an isolated certified block to raw source',
    () async {
      const cases = <(String, String)>[
        ('pla¦in', '*'),
        ('pla¦in', '['),
        ('**bo¦ld**', '_'),
        ('**bo¦ld**', ']'),
        ('_e¦m_', '`'),
        ('_e¦m_', '~'),
        ('`co¦de`', '['),
        ('[la¦bel](https://example.test)', ']'),
        ('[la¦bel](https://example.test)', '>'),
        ('- it¦em', '~'),
        ('> qu¦ote', '>'),
        ('> qu¦ote', '*'),
        ('*op¦en', '_'),
        (r'escaped \¦* literal', 'a'),
        (r'escaped \¦* literal', '`'),
      ];
      for (final (subject, character) in cases) {
        final probe = await LiveEditorTransitionProbe.open(
          '**sentinel**\n\n$subject\n',
          libraryPath: libraryPath!,
        );
        try {
          final trace = probe.typeText(character).single;
          for (final sample in trace.observableStates) {
            expect(
              sample.presentation,
              isNot(contains('**sentinel**')),
              reason: '$subject + $character sample ${sample.sequence}',
            );
            expect(
              sample.rows
                  .expand((row) => row.runs)
                  .any(
                    (run) =>
                        run.text == 'sentinel' && run.styles.contains('strong'),
                  ),
              isTrue,
              reason: '$subject + $character demoted the certified sentinel',
            );
          }
          await probe.expectHealthy();
          await probe.expectConvergesWithCleanRebuild();
        } finally {
          await probe.close();
        }
      }
    },
    skip: libraryPath == null,
  );

  test(
    'structural Return plus an immediate successor remains one exact lineage',
    () async {
      const cases = <(String, String)>[
        ('Paragraph.¦\n1. item\n', 'Paragraph.\n\nNext¦\n1. item\n'),
        ('## Heading¦', '## Heading\n\nNext¦'),
        ('- item¦', '- item\n- Next¦'),
        ('9) item¦', '9) item\n10) Next¦'),
        ('- [x] done¦', '- [x] done\n- [ ] Next¦'),
        ('> quote¦', '> quote\n> Next¦'),
      ];
      for (final (initial, expected) in cases) {
        final probe = await LiveEditorTransitionProbe.open(
          initial,
          libraryPath: libraryPath!,
        );
        try {
          probe.pressReturn();
          final successorCallbackStarted =
              DateTime.now().microsecondsSinceEpoch;
          probe.typeText('Next');
          final successorCallbackReturned =
              DateTime.now().microsecondsSinceEpoch;
          await probe.expectSourceAndCaret(expected);
          final successorReceipts =
              probe.controller.sourceEditPerformanceReceipts;
          expect(successorReceipts, hasLength(4));
          for (final receipt in successorReceipts) {
            expect(
              receipt.acceptedAtEpochMicros,
              inInclusiveRange(
                successorCallbackStarted,
                successorCallbackReturned,
              ),
              reason:
                  'a deferred successor must retain its platform callback '
                  'acceptance instead of adopting its later promotion time',
            );
            expect(receipt.editorSyncMicros, greaterThanOrEqualTo(0));
          }
          await probe.expectHealthy();
          await probe.expectConvergesWithCleanRebuild();
        } finally {
          await probe.close();
        }
      }
    },
    skip: libraryPath == null,
  );

  test(
    'Backspace join plus an immediate successor remains exact',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        'alpha\n\n¦beta\n',
        libraryPath: libraryPath!,
      );
      try {
        probe.pressBackspace();
        probe.typeText('X');
        await probe.expectSourceAndCaret('alphaX¦beta\n');
        await probe.expectHealthy();
        await probe.expectConvergesWithCleanRebuild();
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'deferred structural command retains callback timing across edit lanes',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        '- parent\n  - child¦',
        libraryPath: libraryPath!,
      );
      try {
        probe.pressReturn();
        final successorCallbackStarted = DateTime.now().microsecondsSinceEpoch;
        probe.pressReturn();
        final successorCallbackReturned = DateTime.now().microsecondsSinceEpoch;
        await probe.expectSourceAndCaret('- parent\n  - child\n- ¦');
        final semanticReceipts =
            probe.controller.semanticEditPerformanceReceipts;
        final sourceReceipts = probe.controller.sourceEditPerformanceReceipts;
        expect(semanticReceipts.length + sourceReceipts.length, 2);
        final successorAcceptedAt = semanticReceipts.length == 2
            ? semanticReceipts.last.acceptedAtEpochMicros
            : sourceReceipts.last.acceptedAtEpochMicros;
        final successorCallbackMicros = semanticReceipts.length == 2
            ? semanticReceipts.last.platformCallbackMicros
            : sourceReceipts.last.editorSyncMicros;
        expect(
          successorAcceptedAt,
          inInclusiveRange(successorCallbackStarted, successorCallbackReturned),
        );
        expect(successorCallbackMicros, greaterThanOrEqualTo(0));
        await probe.expectHealthy();
        await probe.expectConvergesWithCleanRebuild();
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );

  test(
    'escaped punctuation edit converges with a clean rebuild',
    () async {
      final probe = await LiveEditorTransitionProbe.open(
        '**sentinel**\n\nescaped \\¦* literal\n',
        libraryPath: libraryPath!,
      );
      try {
        probe.typeText('a');
        await probe.expectSourceAndCaret(
          '**sentinel**\n\nescaped \\a¦* literal\n',
        );
        await probe.expectHealthy();
        await probe.expectConvergesWithCleanRebuild();
      } finally {
        await probe.close();
      }
    },
    skip: libraryPath == null,
  );
}
