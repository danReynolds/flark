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
}
