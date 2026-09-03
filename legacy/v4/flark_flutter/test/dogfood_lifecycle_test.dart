import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// This gate mounts the exact candidate workbench and imports its real preset.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';
// ignore: avoid_relative_lib_imports
import '../example/lib/main.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'Product Tour relaunch starts pristine after edited native state closes',
    (tester) async {
      await tester.binding.setSurfaceSize(const Size(1569, 906));
      addTearDown(() => tester.binding.setSurfaceSize(null));
      expect(
        FlarkNativeDocument.inspectGlobalLiveState(
          libraryPath: libraryPath,
        ).isEmpty,
        isTrue,
      );

      final pristine = buildDogfoodDocument(DogfoodDocumentPreset.productTour);
      final first = await _launchDogfoodApp(tester, libraryPath!);
      await _pumpUntil(
        tester,
        () => first.pendingEdits == 0 && first.semanticsCurrent,
      );

      final insertion = pristine.indexOf('locally.') + 'locally.'.length;
      final paragraph = first.rows.firstWhere(
        (row) =>
            row.editableUtf16 != null &&
            row.editableUtf16!.start <= insertion &&
            insertion <= row.editableUtf16!.end,
      );
      first.activateRow(paragraph, insertion);
      first.replaceSelection('x');
      await _pumpUntil(
        tester,
        () => first.pendingEdits == 0 && first.semanticsCurrent,
      );
      expect(await tester.runAsync(first.readSource), contains('locally.x'));
      expect(await tester.runAsync(first.undo), isTrue);
      await _pumpUntil(
        tester,
        () => first.pendingEdits == 0 && first.semanticsCurrent,
      );
      expect(await tester.runAsync(first.readSource), pristine);

      final task = first.rows.firstWhere(
        (row) => row.listItem?.taskChecked == false,
      );
      expect(
        await tester.runAsync(() => first.toggleTaskChecked(task)),
        isTrue,
      );
      await _pumpUntil(
        tester,
        () => first.pendingEdits == 0 && first.semanticsCurrent,
      );
      final editedBeforeClose = await tester.runAsync(first.readSource);
      expect(editedBeforeClose, contains('- [x] An unchecked task'));
      expect(first.viewportPageIndex, 0);
      expect(first.lastError, isNull);
      expect(first.resyncCount, 0);

      await tester.pumpWidget(const SizedBox.shrink());
      await _waitForGlobalNativeClose(tester, libraryPath);

      final second = await _launchDogfoodApp(tester, libraryPath);
      await _pumpUntil(
        tester,
        () => second.pendingEdits == 0 && second.semanticsCurrent,
      );
      expect(await tester.runAsync(second.readSource), pristine);
      expect(second.viewportPageIndex, 0);
      expect(second.globalSelectionBase, 0);
      expect(second.globalSelectionExtent, 0);
      expect(second.pendingEdits, 0);
      expect(second.semanticsCurrent, isTrue);
      expect(
        second.rows.any((row) => row.listItem?.taskChecked == false),
        isTrue,
      );
      expect(
        second.rows.any((row) => row.listItem?.taskChecked == true),
        isTrue,
      );
      expect(second.lastError, isNull);
      expect(second.resyncCount, 0);

      await tester.pumpWidget(const SizedBox.shrink());
      await _waitForGlobalNativeClose(tester, libraryPath);
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

Future<FlarkEditorController> _launchDogfoodApp(
  WidgetTester tester,
  String libraryPath,
) async {
  await tester.pumpWidget(
    FlarkDogfoodApp(key: UniqueKey(), libraryPath: libraryPath),
  );
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (find.byType(FlarkEditor).evaluate().isEmpty &&
      DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 2)),
    );
    await tester.pump(const Duration(milliseconds: 2));
  }
  expect(find.byType(FlarkEditor), findsOneWidget);
  await tester.pump();
  return tester.widget<FlarkEditor>(find.byType(FlarkEditor)).controller;
}

Future<void> _waitForGlobalNativeClose(
  WidgetTester tester,
  String libraryPath,
) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  var inspection = FlarkNativeDocument.inspectGlobalLiveState(
    libraryPath: libraryPath,
  );
  while (!inspection.isEmpty && DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 2)),
    );
    await tester.pump(const Duration(milliseconds: 2));
    inspection = FlarkNativeDocument.inspectGlobalLiveState(
      libraryPath: libraryPath,
    );
  }
  expect(
    inspection.isEmpty,
    isTrue,
    reason:
        'close leaked sessions=${inspection.liveSessions} '
        'transactions=${inspection.liveTransactions} '
        'continuations=${inspection.liveContinuations} '
        'anchors=${inspection.liveAnchors} '
        'history=${inspection.liveHistoryTokens}',
  );
}

Future<void> _pumpUntil(WidgetTester tester, bool Function() predicate) async {
  final deadline = DateTime.now().add(const Duration(seconds: 30));
  while (!predicate() && DateTime.now().isBefore(deadline)) {
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 2)),
    );
    await tester.pump(const Duration(milliseconds: 2));
  }
  expect(predicate(), isTrue);
}
