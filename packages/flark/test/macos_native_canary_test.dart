import 'dart:io';

import 'package:flutter_test/flutter_test.dart';

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
      expect(
        syntax.paintedPresentations,
        everyElement(isNot(contains('**sentinel**'))),
      );

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

      const wrappedSource = '''# Flark dogfood

This is the real **Rust → Dart → Flutter** editor path. Use it like an editor,
not a static Markdown preview. Certified Markdown stays rendered while focused;
only incomplete or temporarily pending syntax becomes exact source locally.

# Start here
''';
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
      await driver.activateAtUtf16(
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
    },
    skip: enabled
        ? false
        : 'requires macOS, FLARK_CANARY_APP_EXECUTABLE, and native library',
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
    snapshot.paintedCaretIdentities,
    everyElement(isTrue),
    reason: driver.debugLastReceipt,
  );
}
