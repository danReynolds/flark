import 'dart:io';

import 'package:flark/flark.dart';
import 'package:flark/src/render_surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final libraryPath = Platform.environment['FLARK_V4_LIBRARY_PATH'];

  testWidgets(
    'read-only surface scrolls by touch without entering editing state',
    (tester) async {
      final source = List.generate(
        12,
        (index) => 'Read-only paragraph $index.\n\n',
      ).join();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(source, libraryPath: libraryPath!),
      ))!;
      await tester.runAsync(controller.continueParsing);
      final firstOffset = controller.rows.first.editableUtf16!.start;

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: Center(
            child: SizedBox(
              width: 420,
              height: 240,
              child: FlarkMarkdownView(
                controller: controller,
                padding: EdgeInsets.zero,
              ),
            ),
          ),
        ),
      );
      await tester.pump();
      final surface = tester.renderObject<RenderFlarkSurface>(
        find.byType(FlarkRenderSurfaceWidget),
      );
      final before = surface.debugLocalPositionForSourceUtf16(firstOffset);
      expect(before, isNotNull);

      final drag = await tester.startGesture(
        const Offset(400, 300),
        kind: PointerDeviceKind.touch,
      );
      for (var step = 0; step < 3; step += 1) {
        await drag.moveBy(const Offset(0, -30));
      }
      await drag.up();
      await tester.pump();

      final after = surface.debugLocalPositionForSourceUtf16(firstOffset);
      expect(after == null || after.dy < before!.dy - 40, isTrue);
      expect(find.byType(EditableText), findsNothing);
      expect(controller.pendingEdits, 0);
      expect(await tester.runAsync(controller.readSource), source);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
    },
    skip: libraryPath == null,
  );

  testWidgets(
    'read-only semantics expose visible structure without edit actions',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final controller = (await tester.runAsync(
        () => FlarkEditorController.open(
          '# Heading\n\n- [x] done\n\n'
          '${List.generate(20, (index) => 'Body $index.\n\n').join()}'
          'Offscreen sentinel.\n',
          libraryPath: libraryPath!,
        ),
      ))!;
      await tester.runAsync(controller.continueParsing);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: SizedBox(
            width: 420,
            height: 180,
            child: FlarkMarkdownView(controller: controller),
          ),
        ),
      );
      await tester.pump();

      final headingFinder = find.semantics.byLabel('Heading');
      final taskFinder = find.semantics.byLabel('done');
      expect(headingFinder, findsOne);
      expect(taskFinder, findsOne);
      expect(
        headingFinder.evaluate().single,
        isSemantics(label: 'Heading', isHeader: true),
      );
      expect(
        taskFinder.evaluate().single,
        isSemantics(
          label: 'done',
          hasCheckedState: true,
          isChecked: true,
          hasTapAction: false,
        ),
      );
      expect(find.semantics.byLabel('Offscreen sentinel.'), findsNothing);

      await tester.pumpWidget(const SizedBox.shrink());
      await tester.runAsync(controller.close);
      semantics.dispose();
    },
    skip: libraryPath == null,
  );
}
