import 'dart:io';

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  group(
    'editor parse driver',
    () {
      late FlarkCoreDocument document;
      late FlarkCoreEditorSession session;
      late FlarkEditorCoordinator coordinator;
      late FlarkEditorParseDriver driver;

      Future<void> open(String source) async {
        document = await FlarkCoreDocument.open(
          source,
          libraryPath: libraryPath!,
        );
        session = FlarkCoreEditorSession(document);
        coordinator = FlarkEditorCoordinator();
        driver = FlarkEditorParseDriver(
          document: document,
          session: session,
          coordinator: coordinator,
        );
        addTearDown(() async {
          await session.dispose();
          await document.dispose();
        });
      }

      test('advances native parsing to one current publication', () async {
        await open('base\n');

        final step = await driver.next();

        expect(step, isA<FlarkEditorParseReadyPublication>());
        final publication = step as FlarkEditorParseReadyPublication;
        expect(publication.editGeneration, 0);
        expect(document.isReady, isTrue);
        expect(driver.accepts(publication), isTrue);
      });

      test('rejects a publication after a newer edit generation', () async {
        await open('base\n');
        final publication =
            await driver.next() as FlarkEditorParseReadyPublication;

        final newer = coordinator.admitCommand(
          FlarkEditorCommandKind.sourceEdit,
          publishSourceImmediately: true,
        );

        expect(driver.accepts(publication), isFalse);
        coordinator.completeCommand(newer);
      });

      test('publications are bound to their issuing driver', () async {
        await open('base\n');
        final publication =
            await driver.next() as FlarkEditorParseReadyPublication;
        final stranger = FlarkEditorParseDriver(
          document: document,
          session: session,
          coordinator: coordinator,
        );

        expect(() => stranger.accepts(publication), throwsStateError);
      });

      test(
        'edit publication receipt validates one installed current viewport',
        () async {
          await open('base\n');
          final publication = await driver.awaitEditPublication(
            editGeneration: 0,
            allowExactPending: false,
          );
          final viewport = await document.queryViewport(
            endByte: document.sourceByteLength,
            maxRows: 32,
          );
          final stranger = FlarkEditorParseDriver(
            document: document,
            session: session,
            coordinator: coordinator,
          );

          expect(publication, isNotNull);
          expect(
            () => stranger.adoptEditPublication(publication!, viewport),
            throwsStateError,
          );
          expect(driver.adoptEditPublication(publication!, viewport), isTrue);
          expect(
            () => driver.adoptEditPublication(publication, viewport),
            throwsStateError,
          );
        },
      );

      test('edit publication receipt rejects a newer generation', () async {
        await open('base\n');
        final publication = (await driver.awaitEditPublication(
          editGeneration: 0,
          allowExactPending: false,
        ))!;
        final viewport = await document.queryViewport(
          endByte: document.sourceByteLength,
          maxRows: 32,
        );
        final newer = coordinator.admitCommand(
          FlarkEditorCommandKind.sourceEdit,
          publishSourceImmediately: true,
        );

        expect(driver.adoptEditPublication(publication, viewport), isFalse);
        coordinator.completeCommand(newer);
      });

      test('cannot certify an already stale edit generation', () async {
        await open('base\n');
        final newer = coordinator.admitCommand(
          FlarkEditorCommandKind.sourceEdit,
          publishSourceImmediately: true,
        );

        expect(
          await driver.awaitEditPublication(
            editGeneration: 0,
            allowExactPending: false,
          ),
          isNull,
        );
        coordinator.completeCommand(newer);
      });

      test('cannot advance after the coordinator starts closing', () async {
        await open('base\n');
        coordinator.beginClosing();

        expect(await driver.next(), isA<FlarkEditorParseStopped>());
      });
    },
    skip: libraryPath == null
        ? 'Set FLARK_V4_LIBRARY_PATH to run native editor tests.'
        : false,
  );
}
