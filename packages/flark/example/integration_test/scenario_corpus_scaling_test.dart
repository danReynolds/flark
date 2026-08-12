import 'dart:convert';
import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:integration_test/integration_test.dart';

void main() {
  IntegrationTestWidgetsFlutterBinding.ensureInitialized();
  const countSpecification = String.fromEnvironment(
    'FLARK_SCENARIO_SCALE_COUNTS',
  );

  testWidgets(
    'portable scenario corpus surface scaling experiment',
    (tester) async {
      const configuredLibrary = String.fromEnvironment('FLARK_V4_LIBRARY_PATH');
      final libraryPath = configuredLibrary.isNotEmpty
          ? configuredLibrary
          : File(
              '../../../native/comrak_bridge/target/release/libflark_abi.dylib',
            ).absolute.path;

      for (final count in _parseCounts(countSpecification)) {
        final corpusWatch = Stopwatch()..start();
        final caseMicros = <int>[];
        for (var index = 0; index < count; index += 1) {
          if (index > 0 && index % 25 == 0) {
            print(
              'FLARK_SCENARIO_SCALE_PROGRESS ${jsonEncode(<String, Object?>{'cases': count, 'completed': index, 'elapsedMs': corpusWatch.elapsedMilliseconds})}',
            );
          }
          final caseWatch = Stopwatch()..start();
          final beforeCaret = '# Surface scale $index\n\nAlpha beta';
          const afterCaret = ' gamma.\n';
          final initialSource = '$beforeCaret$afterCaret';
          final expectedSource = '${beforeCaret}X\n\nY$afterCaret';
          final controller = await FlarkEditorController.open(
            initialSource,
            libraryPath: libraryPath,
          );
          try {
            await controller.continueParsing();
            await tester.pumpWidget(
              Directionality(
                textDirection: TextDirection.ltr,
                child: SizedBox.expand(
                  child: FlarkEditor(controller: controller),
                ),
              ),
            );
            await tester.pump();

            final caret = beforeCaret.length;
            final row = controller.rows.firstWhere(
              (candidate) =>
                  candidate.editableUtf16 != null &&
                  candidate.editableUtf16!.start <= caret &&
                  caret <= candidate.editableUtf16!.end,
            );
            controller.activateRow(row, caret);
            await tester.pump();

            var platformValue = controller.inputValue;
            platformValue = _insert(controller, platformValue, 'X');
            platformValue = _insertReturn(controller, platformValue);
            platformValue = _insert(controller, platformValue, 'Y');
            await tester.runAsync(() => _settle(controller));
            await tester.pump();

            expect(await controller.readSource(), expectedSource);
            expect(controller.globalCaretOffset, beforeCaret.length + 4);
            expect(controller.resyncCount, 0);
            expect(controller.lastError, isNull);
            expect(controller.status, isNot(FlarkEditorStatus.faulted));
            expect(find.byType(FlarkEditor), findsOneWidget);
            expect(
              controller.rows
                  .map(controller.surfaceRow)
                  .map((row) => row.text)
                  .join('\n'),
              isNot(contains('<empty>')),
            );
          } finally {
            await tester.pumpWidget(const SizedBox.shrink());
            await controller.close();
          }
          caseWatch.stop();
          caseMicros.add(caseWatch.elapsedMicroseconds);
        }
        corpusWatch.stop();
        caseMicros.sort();
        final result = <String, Object?>{
          'runner': 'flutter-surface/${Platform.operatingSystem}',
          'cases': count,
          'elapsedMs': corpusWatch.elapsedMilliseconds,
          'casesPerSecond':
              count /
              (corpusWatch.elapsedMicroseconds /
                  Duration.microsecondsPerSecond),
          'caseP50Ms':
              _percentile(caseMicros, 0.50) /
              Duration.microsecondsPerMillisecond,
          'caseP95Ms':
              _percentile(caseMicros, 0.95) /
              Duration.microsecondsPerMillisecond,
          'caseMaxMs': caseMicros.last / Duration.microsecondsPerMillisecond,
        };
        print('FLARK_SCENARIO_SCALE_RESULT ${jsonEncode(result)}');
      }
    },
    skip: countSpecification.isEmpty,
  );
}

TextEditingValue _insert(
  FlarkEditorController controller,
  TextEditingValue before,
  String text,
) {
  final offset = before.selection.extentOffset;
  final delta = TextEditingDeltaInsertion(
    oldText: before.text,
    textInserted: text,
    insertionOffset: offset,
    selection: TextSelection.collapsed(offset: offset + text.length),
    composing: TextRange.empty,
  );
  controller.applyDeltas([delta]);
  return delta.apply(before);
}

TextEditingValue _insertReturn(
  FlarkEditorController controller,
  TextEditingValue before,
) {
  final offset = before.selection.extentOffset;
  final delta = TextEditingDeltaInsertion(
    oldText: before.text,
    textInserted: '\n',
    insertionOffset: offset,
    selection: TextSelection.collapsed(offset: offset + 1),
    composing: TextRange.empty,
  );
  controller.applyDeltas([delta]);
  controller.observePlatformNewlineAction();
  return delta.apply(before);
}

Future<void> _settle(FlarkEditorController controller) async {
  final deadline = DateTime.now().add(const Duration(seconds: 5));
  while (controller.pendingEdits != 0 && DateTime.now().isBefore(deadline)) {
    await Future<void>.delayed(const Duration(milliseconds: 1));
  }
  if (controller.pendingEdits == 0) await controller.continueParsing();
  if (controller.pendingEdits != 0) {
    throw StateError('surface scaling case did not settle');
  }
  if (controller.lastError case final error?) throw error;
}

List<int> _parseCounts(String specification) {
  final counts = specification.split(',').map(int.parse).toList();
  if (counts.isEmpty || counts.any((count) => count <= 0)) {
    throw FormatException('scenario scale counts must all be positive');
  }
  return counts;
}

int _percentile(List<int> sortedValues, double percentile) {
  final index = ((sortedValues.length - 1) * percentile).round();
  return sortedValues[index];
}
