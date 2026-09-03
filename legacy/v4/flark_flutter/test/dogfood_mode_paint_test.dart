import 'dart:convert';
import 'dart:io';

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

// The mode journey uses the exact document exposed by the candidate app.
// ignore: avoid_relative_lib_imports
import '../example/lib/dogfood_documents.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'Product Tour Edit Read Edit preserves one rendered source and selection',
    (tester) async {
      final source = buildDogfoodDocument(DogfoodDocumentPreset.productTour);
      final caret = source.indexOf('editor path') + 3;
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final row = controller.rows.firstWhere(
        (candidate) =>
            candidate.editableUtf16 != null &&
            candidate.editableUtf16!.start <= caret &&
            caret <= candidate.editableUtf16!.end,
      );
      // Activation and its serialized selection wait must share one async
      // zone. Splitting them across Flutter's fake-async test body and
      // runAsync strands the command continuation in the originating zone.
      await tester.runAsync(() async {
        controller.activateRow(row, caret);
        await controller.debugWaitForMutationSettled();
      });
      await tester.binding.setSurfaceSize(const Size(900, 700));

      final firstEditPaints = <FlarkSurfacePaintObservation>[];
      final readPaints = <FlarkSurfacePaintObservation>[];
      final secondEditPaints = <FlarkSurfacePaintObservation>[];
      try {
        await tester.pumpWidget(
          _modeSurface(
            controller,
            readOnly: false,
            observer: firstEditPaints.add,
          ),
        );
        await tester.pump();
        await tester.pump();
        expect(find.byType(FlarkEditor), findsOneWidget);
        final firstEdit = firstEditPaints.last;
        _expectCurrentPaint(firstEdit, source, caret, editing: true);
        final editManifest = _renderManifest(firstEdit);

        await tester.pumpWidget(
          _modeSurface(controller, readOnly: true, observer: readPaints.add),
        );
        await tester.pump();
        expect(find.byType(FlarkMarkdownView), findsOneWidget);
        expect(find.byType(FlarkEditor), findsNothing);
        final read = readPaints.last;
        _expectCurrentPaint(read, source, caret, editing: false);
        expect(_renderManifest(read), editManifest);
        expect(controller.globalSelectionBase, caret);
        expect(controller.globalSelectionExtent, caret);

        await tester.pumpWidget(
          _modeSurface(
            controller,
            readOnly: false,
            observer: secondEditPaints.add,
          ),
        );
        await tester.pump();
        await tester.pump();
        expect(find.byType(FlarkEditor), findsOneWidget);
        final secondEdit = secondEditPaints.last;
        _expectCurrentPaint(secondEdit, source, caret, editing: true);
        expect(_renderManifest(secondEdit), editManifest);
        expect(controller.sourceGeneration, firstEdit.sourceGeneration);
        expect(await tester.runAsync(controller.readSource), source);
        expect(controller.lastError, isNull);
        expect(controller.resyncCount, 0);
      } finally {
        await tester.pumpWidget(const SizedBox.shrink());
        await tester.binding.setSurfaceSize(null);
        await tester.runAsync(controller.close);
      }
    },
    skip: libraryPath == null,
    timeout: const Timeout(Duration(minutes: 2)),
  );
}

Widget _modeSurface(
  FlarkEditorController controller, {
  required bool readOnly,
  required ValueChanged<FlarkSurfacePaintObservation> observer,
}) => Directionality(
  textDirection: TextDirection.ltr,
  child: SizedBox.expand(
    child: readOnly
        ? FlarkMarkdownView(
            controller: controller,
            padding: EdgeInsets.zero,
            debugPaintObserver: observer,
          )
        : FlarkEditor(
            controller: controller,
            autofocus: true,
            padding: EdgeInsets.zero,
            debugPaintObserver: observer,
          ),
  ),
);

void _expectCurrentPaint(
  FlarkSurfacePaintObservation paint,
  String source,
  int caret, {
  required bool editing,
}) {
  expect(paint.visibleSource, source);
  expect(paint.canonicalSelectionBaseUtf16, caret);
  expect(paint.canonicalSelectionExtentUtf16, caret);
  expect(
    paint.presentation.replaceAll('**unfinished', ''),
    isNot(contains('**')),
  );
  final strong = paint.rows
      .expand((row) => row.runs)
      .where((run) => run.text == 'Rust → Dart → Flutter')
      .toList(growable: false);
  expect(strong, isNotEmpty);
  expect(
    strong.every((run) => run.styles.contains(FlarkSurfaceInlineStyle.strong)),
    isTrue,
  );
  if (editing) {
    expect(paint.caretRect, isNotNull);
    expect(paint.caretSourceUtf16, caret);
    expect(paint.caretDisplayUtf16, isNotNull);
    expect(paint.rows.where((row) => row.active), hasLength(1));
  } else {
    expect(paint.caretRect, isNull);
    expect(paint.caretSourceUtf16, isNull);
    expect(paint.caretDisplayUtf16, isNull);
    expect(paint.selectionRects, isEmpty);
    expect(paint.rows.every((row) => !row.active), isTrue);
  }
}

String _renderManifest(FlarkSurfacePaintObservation paint) => jsonEncode(
  paint.rows
      .map(
        (row) => {
          'ordinal': row.ordinal,
          'kind': row.kind,
          'headingLevel': row.headingLevel,
          'blockQuoteDepth': row.blockQuoteDepth,
          'listItem': row.listItem,
          'table': row.table,
          'leadingText': row.leadingText,
          'sourceStart': row.sourceUtf16Start,
          'fragmentStart': row.fragmentStart,
          'fragmentEnd': row.fragmentEnd,
          'text': row.text,
          'blockStyle': row.resolvedBlockStyle.toString(),
          'runs': row.runs
              .map(
                (run) => {
                  'text': run.text,
                  'sourceStart': run.sourceUtf16Start,
                  'sourceEnd': run.sourceUtf16End,
                  'sourceExact': run.sourceExact,
                  'styles': run.styles.map((style) => style.name).toList()
                    ..sort(),
                  'resolvedStyle': run.resolvedStyle.toString(),
                },
              )
              .toList(),
        },
      )
      .toList(),
);
