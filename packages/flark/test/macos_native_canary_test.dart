import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

// The native qualification lane must exercise the app's real default source.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';
import 'support/macos_native_canary_driver.dart';

void main() {
  final appExecutable = Platform.environment['FLARK_CANARY_APP_EXECUTABLE'];
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];
  final enabled =
      Platform.isMacOS && appExecutable != null && libraryPath != null;

  test(
    'macOS routes the native editing canaries without faults or visual relay',
    () async {
      final driver = MacosNativeCanaryDriver(
        appExecutable: appExecutable!,
        libraryPath: libraryPath!,
        actuatorScript: 'tool/live_editor_macos_canary.swift',
      );
      addTearDown(driver.close);

      const syntaxSource = '**sentinel**\n\nplain\n';
      await driver.reset(id: 'syntax-key-routing', source: syntaxSource);
      await driver.activateAtUtf16(syntaxSource.indexOf('plain') + 5);
      await driver.typeText('*');
      final syntax = await driver.settle();
      _expectHealthy(syntax, driver);
      expect(syntax.source, '**sentinel**\n\nplain*\n');
      expect(syntax.paintedPresentations, isNotEmpty);
      expect(
        syntax.paintedPresentations,
        everyElement(isNot(contains('**sentinel**'))),
      );
      expect(
        syntax.paintedStyledTexts,
        everyElement(contains('strong:sentinel')),
        reason: driver.debugLastReceipt,
      );

      const deadKeySource = 'caf\n';
      await driver.reset(id: 'dead-key-composition', source: deadKeySource);
      await driver.activateAtUtf16(3);
      await driver.pressKey('acuteE');
      final deadKey = await driver.settle();
      _expectHealthy(deadKey, driver);
      expect(deadKey.source, 'café\n');
      expect(deadKey.selectionBaseUtf16, 4);
      expect(deadKey.selectionExtentUtf16, 4);

      const blankSource = 'alpha\n\n## next\n';
      await driver.reset(id: 'return-backspace-routing', source: blankSource);
      await driver.activateAtUtf16(5);
      await driver.pressKey('enter');
      final firstReturn = await driver.settle();
      await driver.pressKey('enter');
      await driver.settle();
      await driver.pressKey('backspace');
      final backspace = await driver.settle();
      _expectHealthy(backspace, driver);
      expect(backspace.source, firstReturn.source);
      expect(backspace.selectionBaseUtf16, firstReturn.selectionBaseUtf16);
      expect(backspace.selectionExtentUtf16, firstReturn.selectionExtentUtf16);

      const clipboardSource = 'alpha beta\n';
      await driver.reset(
        id: 'pointer-clipboard-history-routing',
        source: clipboardSource,
      );
      await driver.selectSourceRange(base: 0, extent: 5);
      await driver.pressKey('cut');
      final cut = await driver.settle();
      _expectHealthy(cut, driver);
      expect(cut.source, ' beta\n');
      await driver.pressKey('undo');
      final undo = await driver.settle();
      _expectHealthy(undo, driver);
      expect(undo.source, clipboardSource);

      final longSource = List.generate(
        80,
        (index) => 'Paragraph $index with enough text to render.\n\n',
      ).join();
      await driver.reset(id: 'scroll-does-not-select', source: longSource);
      await driver.activateAtUtf16(2);
      await driver.scrollBy(420);
      final scrolled = await driver.settle();
      _expectHealthy(scrolled, driver);
      expect(scrolled.scrollOffset, greaterThan(0));
      expect(scrolled.selectionBaseUtf16, 2);
      expect(scrolled.selectionExtentUtf16, 2);

      final wrappedSource = buildDogfoodDocument(
        DogfoodDocumentPreset.productTour,
      );
      await driver.reset(id: 'wrapped-caret-stability', source: wrappedSource);
      await driver.activateAtUtf16(
        wrappedSource.indexOf('This'),
        windowWidth: 1569,
        windowHeight: 906,
      );
      await driver.typeText('keepwhat');
      await driver.settle();
      await driver.pressKey('enter');
      final prepared = await driver.settle();
      final wrappedCaret =
          prepared.source.indexOf('locally.') + 'locally.'.length;
      final wrappedStart = await driver.activateAtUtf16(
        wrappedCaret,
        windowWidth: 1569,
        windowHeight: 906,
      );
      const successor = ' Testing is somewhat useful but lik';
      await driver.typeText(
        successor,
        cadence: const Duration(milliseconds: 80),
      );
      final wrapped = await driver.settle();
      _expectHealthy(wrapped, driver);
      expect(
        wrapped.source,
        prepared.source.replaceRange(wrappedCaret, wrappedCaret, successor),
      );
      expect(wrapped.selectionBaseUtf16, wrappedCaret + successor.length);
      expect(wrapped.selectionExtentUtf16, wrappedCaret + successor.length);
      expect(wrapped.paintedSourceGenerations, isNotEmpty);
      for (var index = 0; index < successor.length; index += 1) {
        final generation = wrappedStart.sourceGeneration + index + 1;
        final expectedLocalPresentation =
            'locally.${successor.substring(0, index + 1)}';
        final frames = <int>[
          for (
            var frame = 0;
            frame < wrapped.paintedSourceGenerations.length;
            frame += 1
          )
            if (wrapped.paintedSourceGenerations[frame] == generation) frame,
        ];
        expect(
          frames,
          isNotEmpty,
          reason: 'generation $generation not painted',
        );
        for (final frame in frames) {
          expect(
            wrapped.paintedPresentations[frame],
            contains(expectedLocalPresentation),
            reason:
                'generation $generation did not paint its accepted local text',
          );
          expect(
            wrapped.paintedPresentations[frame],
            allOf(
              startsWith('Flark dogfood\nkeepwhat\n'),
              contains('GFM projection'),
              contains('Surface │ Authority │ State'),
            ),
            reason: 'generation $generation omitted a visible projected anchor',
          );
          expect(
            wrapped.paintedSelectionBases[frame],
            wrappedCaret + index + 1,
          );
          expect(
            wrapped.paintedSelectionExtents[frame],
            wrappedCaret + index + 1,
          );
          expect(wrapped.paintedCaretSources[frame], wrappedCaret + index + 1);
          expect(wrapped.paintedCaretDisplays[frame], isNotNull);
          expect(
            wrapped.paintedVisibleSources[frame],
            prepared.source.replaceRange(
              wrappedCaret,
              wrappedCaret,
              successor.substring(0, index + 1),
            ),
          );
        }
      }
      expect(
        wrapped.paintedPresentations,
        everyElement(isNot(contains('**Rust → Dart → Flutter**'))),
      );
      expect(
        wrapped.paintedStyledTexts,
        everyElement(contains('strong:Rust → Dart → Flutter')),
      );
    },
    skip: enabled
        ? false
        : 'requires macOS, FLARK_CANARY_APP_EXECUTABLE, and native library',
    timeout: const Timeout(Duration(seconds: 180)),
  );
}

void _expectHealthy(
  MacosNativeCanarySnapshot snapshot,
  MacosNativeCanaryDriver driver,
) {
  expect(snapshot.faulted, isFalse, reason: driver.debugLastReceipt);
  expect(snapshot.lastError, isNull, reason: driver.debugLastReceipt);
  expect(snapshot.resyncCount, 0, reason: driver.debugLastReceipt);
  expect(snapshot.lastResyncReason, 'none', reason: driver.debugLastReceipt);
  expect(
    snapshot.paintedPresentations,
    isNotEmpty,
    reason: 'native qualification requires at least one actual paint',
  );
  expect(
    snapshot.paintedCaretIdentities,
    everyElement(isTrue),
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedSourceGenerations.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedSelectionBases.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedSelectionExtents.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedCaretSources.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedCaretDisplays.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedVisibleSources.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
  expect(
    snapshot.paintedStyledTexts.length,
    snapshot.paintedPresentations.length,
    reason: driver.debugLastReceipt,
  );
}
