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
      const punctuation = '*[`~>\\';
      await driver.typeText(punctuation);
      final syntax = await driver.settle();
      _expectHealthy(syntax, driver);
      expect(syntax.source, '**sentinel**\n\nplain$punctuation\n');
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

      const repeatedReturnSource = 'fff';
      await driver.reset(
        id: 'repeated-return-successor-liveness',
        source: repeatedReturnSource,
      );
      await driver.activateAtUtf16(repeatedReturnSource.length);
      const settledReturnSources = <String>[
        'fff\n\n',
        'fff\n\n\n',
        'fff\n\n\n\n',
      ];
      const settledReturnCarets = <int>[5, 6, 7];
      for (var index = 0; index < 3; index += 1) {
        await driver.pressKey('enter');
        final settledReturn = await driver.settle();
        _expectHealthy(settledReturn, driver);
        expect(
          settledReturn.source,
          settledReturnSources[index],
          reason:
              'settled native Return ${index + 1}\n'
              '${driver.debugLastInputEvents}',
        );
        expect(settledReturn.selectionBaseUtf16, settledReturnCarets[index]);
        expect(settledReturn.selectionExtentUtf16, settledReturnCarets[index]);
      }
      final afterReturns = await driver.settle();
      expect(afterReturns.source, settledReturnSources.last);
      expect(afterReturns.selectionBaseUtf16, settledReturnCarets.last);
      expect(afterReturns.selectionExtentUtf16, settledReturnCarets.last);
      await driver.typeText('x');
      final repeatedReturn = await driver.settle();
      _expectHealthy(repeatedReturn, driver);
      expect(repeatedReturn.source, 'fff\n\n\n\nx');
      expect(repeatedReturn.selectionBaseUtf16, 8);
      expect(repeatedReturn.selectionExtentUtf16, 8);

      await driver.reset(
        id: 'rapid-return-successor-liveness',
        source: repeatedReturnSource,
      );
      await driver.activateAtUtf16(repeatedReturnSource.length);
      await driver.repeatKey('enter', count: 3, cadence: Duration.zero);
      final rapidReturns = await driver.settle();
      _expectHealthy(rapidReturns, driver);
      expect(rapidReturns.source, settledReturnSources.last);
      expect(rapidReturns.selectionBaseUtf16, settledReturnCarets.last);
      expect(rapidReturns.selectionExtentUtf16, settledReturnCarets.last);
      await driver.typeText('x', cadence: Duration.zero);
      final rapidSuccessor = await driver.settle();
      _expectHealthy(rapidSuccessor, driver);
      expect(rapidSuccessor.source, 'fff\n\n\n\nx');
      expect(rapidSuccessor.selectionBaseUtf16, 8);
      expect(rapidSuccessor.selectionExtentUtf16, 8);

      const structuralSource = 'Before **bold**.\n';
      await driver.reset(
        id: 'structural-burst-liveness',
        source: structuralSource,
      );
      await driver.activateAtUtf16(structuralSource.length - 1);
      await driver.typeStructuralBursts(
        count: 3,
        cadence: const Duration(milliseconds: 40),
      );
      final structuralBursts = await driver.settle();
      _expectHealthy(structuralBursts, driver);
      expect(structuralBursts.source, 'Before **bold**.\n\nx\n\nx\n\nx\n');
      expect(
        structuralBursts.paintedPresentations,
        everyElement(isNot(contains('**bold**'))),
      );
      expect(
        structuralBursts.paintedStyledTexts,
        everyElement(contains('strong:bold')),
      );

      const navigationSource = 'alpha\n';
      await driver.reset(
        id: 'arrow-then-type-routing',
        source: navigationSource,
      );
      await driver.activateAtUtf16(5);
      await driver.pressKey('left');
      final leftOnce = await driver.settle();
      _expectHealthy(leftOnce, driver);
      expect(leftOnce.selectionBaseUtf16, 4);
      expect(leftOnce.selectionExtentUtf16, 4);
      await driver.pressKey('left');
      final leftTwice = await driver.settle();
      _expectHealthy(leftTwice, driver);
      expect(leftTwice.selectionBaseUtf16, 3);
      expect(leftTwice.selectionExtentUtf16, 3);
      await driver.typeText('X', cadence: Duration.zero);
      final navigatedTyping = await driver.settle();
      _expectHealthy(navigatedTyping, driver);
      expect(navigatedTyping.source, 'alpXha\n');
      expect(navigatedTyping.selectionBaseUtf16, 4);
      expect(navigatedTyping.selectionExtentUtf16, 4);

      const clipboardSource = 'alpha beta\n';
      await driver.reset(
        id: 'pointer-clipboard-history-routing',
        source: clipboardSource,
      );
      await driver.selectSourceRange(base: 0, extent: 5);
      await driver.pressKey('copy');
      final copied = await driver.settle();
      _expectHealthy(copied, driver);
      expect(copied.source, clipboardSource);
      expect(copied.selectionBaseUtf16, 0);
      expect(copied.selectionExtentUtf16, 5);
      await driver.pressKey('cut');
      final cut = await driver.settle();
      _expectHealthy(cut, driver);
      expect(cut.source, ' beta\n');
      await driver.pressKey('paste');
      final pasted = await driver.settle();
      _expectHealthy(pasted, driver);
      expect(pasted.source, clipboardSource);
      await driver.pressKey('undo');
      final undoPaste = await driver.settle();
      _expectHealthy(undoPaste, driver);
      expect(undoPaste.source, ' beta\n');
      await driver.pressKey('undo');
      final undoCut = await driver.settle();
      _expectHealthy(undoCut, driver);
      expect(undoCut.source, clipboardSource);
      await driver.pressKey('redo');
      final redoCut = await driver.settle();
      _expectHealthy(redoCut, driver);
      expect(redoCut.source, ' beta\n');
      await driver.pressKey('redo');
      final redoPaste = await driver.settle();
      _expectHealthy(redoPaste, driver);
      expect(redoPaste.source, clipboardSource);
      await driver.selectSourceRange(base: 6, extent: 10);
      await driver.typeText('gamma', cadence: Duration.zero);
      final replaced = await driver.settle();
      _expectHealthy(replaced, driver);
      expect(replaced.source, 'alpha gamma\n');
      await driver.selectSourceRange(base: 6, extent: 11);
      await driver.pasteText('βeta 👩‍💻');
      final unicodePaste = await driver.settle();
      _expectHealthy(unicodePaste, driver);
      expect(unicodePaste.source, 'alpha βeta 👩‍💻\n');
      await driver.pressKey('undo');
      final undoUnicodePaste = await driver.settle();
      _expectHealthy(undoUnicodePaste, driver);
      expect(undoUnicodePaste.source, 'alpha gamma\n');
      await driver.pressKey('redo');
      final redoUnicodePaste = await driver.settle();
      _expectHealthy(redoUnicodePaste, driver);
      expect(redoUnicodePaste.source, 'alpha βeta 👩‍💻\n');

      final longSource = List.generate(
        80,
        (index) => 'Paragraph $index with enough text to render.\n\n',
      ).join();
      await driver.reset(id: 'scroll-does-not-select', source: longSource);
      await driver.activateAtUtf16(2);
      await driver.scrollBy(420);
      final scrolled = await driver.settle();
      _expectHealthy(scrolled, driver);
      expect(_isLogicallyAfter(scrolled, page: 0, offset: 0), isTrue);
      expect(scrolled.selectionBaseUtf16, 2);
      expect(scrolled.selectionExtentUtf16, 2);
      await driver.scrollBy(420);
      final farther = await driver.settle();
      _expectHealthy(farther, driver);
      expect(_isLogicallyAfterSnapshot(farther, scrolled), isTrue);
      expect(farther.selectionBaseUtf16, 2);
      expect(farther.selectionExtentUtf16, 2);
      await driver.scrollBy(-840);
      final back = await driver.settle();
      _expectHealthy(back, driver);
      expect(_isLogicallyAfterSnapshot(farther, back), isTrue);
      expect(back.selectionBaseUtf16, 2);
      expect(back.selectionExtentUtf16, 2);
      await driver.scrollBy(-840);
      final returned = await driver.settle();
      _expectHealthy(returned, driver);
      expect(_viewportPageIndex(returned), 0);
      expect(returned.scrollOffset, closeTo(0, 1));
      expect(returned.selectionBaseUtf16, 2);
      expect(returned.selectionExtentUtf16, 2);

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

      const rapidTail = ' Rapid native typing stays rendered and responsive.';
      await driver.typeText(rapidTail, cadence: Duration.zero);
      final rapidWrapped = await driver.settle();
      _expectHealthy(rapidWrapped, driver);
      expect(
        rapidWrapped.source,
        wrapped.source.replaceRange(
          wrapped.selectionExtentUtf16,
          wrapped.selectionExtentUtf16,
          rapidTail,
        ),
      );
      expect(
        rapidWrapped.selectionExtentUtf16,
        wrapped.selectionExtentUtf16 + rapidTail.length,
      );
      expect(
        rapidWrapped.paintedPresentations,
        everyElement(isNot(contains('**Rust → Dart → Flutter**'))),
      );
      expect(
        rapidWrapped.paintedStyledTexts,
        everyElement(contains('strong:Rust → Dart → Flutter')),
      );

      final deliveryAcknowledgements = driver
          .inputDeliveryAcknowledgementsSince(0);
      expect(deliveryAcknowledgements, hasLength(30));
      for (final acknowledgement in deliveryAcknowledgements) {
        final baselineOrdinal =
            acknowledgement['baselineInputEventOrdinal']! as int;
        final terminalOrdinal =
            acknowledgement['terminalInputEventOrdinal']! as int;
        final baselineGeneration =
            acknowledgement['baselineSourceGeneration']! as int;
        final terminalGeneration =
            acknowledgement['terminalSourceGeneration']! as int;
        final advance = acknowledgement['expectedGenerationAdvance']! as int;
        expect(terminalOrdinal, greaterThan(baselineOrdinal));
        expect(
          terminalGeneration,
          greaterThanOrEqualTo(baselineGeneration + advance),
        );
        final terminalEvent = acknowledgement['terminalEvent']! as String;
        if (terminalEvent.contains('generation=')) {
          expect(terminalEvent, contains('generation=$terminalGeneration'));
        } else {
          expect(advance, 0);
          expect(terminalEvent, contains('KeyUpEvent'));
        }
      }
      expect(
        deliveryAcknowledgements,
        contains(
          isA<Map<String, Object?>>().having(
            (acknowledgement) => acknowledgement['expectedGenerationAdvance'],
            'full wrapped typing batch',
            successor.length,
          ),
        ),
      );
    },
    skip: enabled
        ? false
        : 'requires macOS, FLARK_CANARY_APP_EXECUTABLE, and native library',
    timeout: const Timeout(Duration(seconds: 180)),
  );
}

bool _isLogicallyAfterSnapshot(
  MacosNativeCanarySnapshot candidate,
  MacosNativeCanarySnapshot baseline,
) => _isLogicallyAfter(
  candidate,
  page: _viewportPageIndex(baseline),
  offset: baseline.scrollOffset,
);

bool _isLogicallyAfter(
  MacosNativeCanarySnapshot candidate, {
  required int page,
  required double offset,
}) {
  final candidatePage = _viewportPageIndex(candidate);
  return candidatePage > page ||
      (candidatePage == page && candidate.scrollOffset > offset);
}

int _viewportPageIndex(MacosNativeCanarySnapshot snapshot) =>
    snapshot.paintReceipts.last['viewportPageIndex']! as int;

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
