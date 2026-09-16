import 'dart:io';
import 'dart:ui' as ui;

import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  testWidgets(
    'mixed block paint has shared outer edges and stable marker gutters',
    (tester) async {
      final source = File(
        '../../test/fixtures/host_block_alignment.md',
      ).readAsStringSync();
      final font = File(
        'lib/assets/fonts/RobotoMono-Regular.otf',
      ).readAsBytesSync();
      for (final family in ['AlignmentTest', flarkCodeFontFamily]) {
        await (FontLoader(
          family,
        )..addFont(Future.value(ByteData.sublistView(font)))).load();
      }
      tester.view.physicalSize = const Size(800, 1400);
      tester.view.devicePixelRatio = 1;
      addTearDown(tester.view.resetPhysicalSize);
      addTearDown(tester.view.resetDevicePixelRatio);
      final c = FlarkController(
        FlarkEditor(createParseBackend(), text: source, caret: 0),
      );
      final boundary = GlobalKey();
      const codeColor = Color(0xffe7edf3),
          railColor = Color(0xff8090a0),
          tableColor = Color(0xff445566),
          bulletColor = Color(0xff2266cc),
          taskColor = Color(0xffcc6622);
      for (final compact in [true, false]) {
        final indent = compact ? 30.0 : 42.0, quote = compact ? 22.0 : 32.0;
        await tester.pumpWidget(
          MaterialApp(
            home: RepaintBoundary(
              key: boundary,
              child: FlarkEditorWidget(
                controller: c,
                showToolbar: false,
                theme: FlarkThemeData(
                  styles: const {
                    FlarkTextRole.body: TextStyle(
                      fontFamily: 'AlignmentTest',
                      fontSize: 16,
                      letterSpacing: 0,
                      color: Color(0xff18212b),
                    ),
                    FlarkTextRole.listMarker: TextStyle(color: bulletColor),
                  },
                  colors: const {
                    FlarkColorRole.canvas: Colors.white,
                    FlarkColorRole.codeBackground: codeColor,
                    FlarkColorRole.quoteRail: railColor,
                    FlarkColorRole.tableBorder: tableColor,
                    FlarkColorRole.taskBorder: taskColor,
                  },
                  metrics: {
                    FlarkMetric.documentPadding: 16,
                    FlarkMetric.listIndent: indent,
                    FlarkMetric.quoteIndent: quote,
                    FlarkMetric.listMarkerGap: 6,
                    FlarkMetric.codePadding: 8,
                    FlarkMetric.codeRadius: 0,
                  },
                ),
              ),
            ),
          ),
        );
        final surface = tester.renderObject<RenderFlarkSurface>(
          find.byType(FlarkSurface),
        );
        Future<Rect> caretAt(String text) async {
          c.command(SetSelection.caret(source.indexOf(text)));
          await tester.pump();
          return surface.caretRect;
        }

        final paragraph = await caretAt('Paragraph.');
        expect(paragraph.left, 16);
        expect((await caretAt('Heading')).left, paragraph.left);
        expect((await caretAt('Quoted')).left, paragraph.left + quote);
        for (final text in ['Task item', 'Bullet item', 'Separate bullet']) {
          expect((await caretAt(text)).left, paragraph.left + indent);
        }
        final taskCaret = await caretAt('Task item');
        final bulletCaret = await caretAt('Bullet item');
        expect(
          (await caretAt('Nested bullet')).left,
          paragraph.left + 2 * indent,
        );
        final code = await caretAt('first');
        expect(code.left, paragraph.left + 8);
        expect((await caretAt('Feature')).left, paragraph.left + 8);

        final paintBounds = await tester.runAsync(() async {
          final render =
              boundary.currentContext!.findRenderObject()!
                  as RenderRepaintBoundary;
          final image = await render.toImage();
          final data = (await image.toByteData(
            format: ui.ImageByteFormat.rawRgba,
          ))!.buffer.asUint8List();
          Rect bounds(Color color, {Rect? region}) {
            var left = image.width, top = image.height, right = -1, bottom = -1;
            final argb = color.toARGB32();
            for (var y = 0; y < image.height; y++) {
              for (var x = 0; x < image.width; x++) {
                if (region != null &&
                    !region.contains(Offset(x.toDouble(), y.toDouble()))) {
                  continue;
                }
                final i = (y * image.width + x) * 4;
                if (data[i] == (argb >> 16 & 255) &&
                    data[i + 1] == (argb >> 8 & 255) &&
                    data[i + 2] == (argb & 255)) {
                  if (x < left) left = x;
                  if (y < top) top = y;
                  if (x > right) right = x;
                  if (y > bottom) bottom = y;
                }
              }
            }
            expect(right, greaterThanOrEqualTo(left), reason: 'Painted $color');
            return Rect.fromLTRB(
              left.toDouble(),
              top.toDouble(),
              right + 1.0,
              bottom + 1.0,
            );
          }

          final result = {
            for (final color in [codeColor, railColor, tableColor])
              color: bounds(color),
            taskColor: bounds(
              taskColor,
              region: Rect.fromLTWH(
                16,
                taskCaret.top,
                indent,
                taskCaret.height,
              ),
            ),
            bulletColor: bounds(
              bulletColor,
              region: Rect.fromLTWH(
                16,
                bulletCaret.top,
                indent,
                bulletCaret.height,
              ),
            ),
          };
          final capture = Platform.environment['FLARK_ALIGNMENT_CAPTURE'];
          if (capture != null && compact) {
            File(capture).writeAsBytesSync(
              (await image.toByteData(
                format: ui.ImageByteFormat.png,
              ))!.buffer.asUint8List(),
            );
          }
          image.dispose();
          return result;
        });
        for (final color in [codeColor, railColor, tableColor]) {
          expect(
            paintBounds![color]!.left,
            paragraph.left,
            reason: '$color outer edge',
          );
        }
        expect(
          paintBounds![taskColor]!.center.dx,
          closeTo(paintBounds[bulletColor]!.center.dx, 1),
        );
        expect(
          code.top - paintBounds[codeColor]!.top - (paragraph.top - 20),
          closeTo(8, 1),
        );
        for (final text in [
          'Quoted',
          'Task item',
          'Bullet item',
          'Separate bullet',
          'Nested bullet',
          'first',
          'Feature',
        ]) {
          final caret = await caretAt(text);
          surface.place(caret.centerLeft);
          expect(c.editor.selection.extent, source.indexOf(text));
          c.command(const InsertText('X'));
          await tester.pump();
          expect(c.text, source.replaceFirst(text, 'X$text'));
          c.command(const Undo());
          await tester.pump();
          expect(c.text, source);
        }
      }
      await tester.pumpWidget(const SizedBox());
      c.dispose();
    },
  );
}
