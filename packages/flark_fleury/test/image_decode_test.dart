import 'dart:async';
import 'dart:typed_data';
import 'package:flark/flark.dart';
import 'package:flark_fleury/flark_fleury.dart';
import 'package:flark_fleury/src/image_decode.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury/fleury_test_support.dart';
import 'package:image/image.dart' as pixels;
import 'package:test/test.dart';

void main() {
  late List<int> png;
  setUpAll(() {
    png = pixels.encodePng(pixels.Image(width: 2048, height: 2048));
  });
  test(
    'decoder queue bounds work and cancellation releases active and queued jobs',
    () async {
      final queue = PreviewDecodeQueue();
      final tokens = List.generate(10, (_) => PreviewCancellation());
      final futures = [
        for (final token in tokens)
          queue
              .decode(Uint8List.fromList(png), token)
              .then<Object>((value) => value, onError: (Object e) => e),
      ];
      expect(queue.active, 2);
      expect(queue.queued, 8);
      await expectLater(
        queue.decode(Uint8List.fromList(png), PreviewCancellation()),
        throwsFormatException,
      );
      for (final token in tokens) {
        token.cancel();
      }
      final results = await Future.wait(futures);
      expect(results, everyElement(isA<PreviewCancelled>()));
      expect(queue.active, 0);
      expect(queue.queued, 0);
      final small = pixels.encodePng(pixels.Image(width: 8, height: 4));
      final result = await queue.decode(small, PreviewCancellation());
      expect((result.image.width, result.image.height), (8, 4));
      expect(pixels.decodePng(result.png)!.width, 8);
    },
  );
  test(
    'real large-image preparation allows typing and first paint before completion',
    () async {
      final queue = PreviewDecodeQueue(), token = PreviewCancellation();
      final tester = FleuryTester(viewportSize: const CellSize(40, 10));
      final editor = FlarkEditor(createParseBackend(), text: 'draft', caret: 5);
      final controller = FlarkFleuryController(editor);
      final focus = FocusNode();
      addTearDown(() {
        token.cancel();
        tester.dispose();
        controller.dispose();
        focus.dispose();
      });
      tester.pumpWidget(
        Theme(
          data: const ThemeData(),
          child: FlarkEditorView(
            controller: controller,
            autofocus: true,
            focusNode: focus,
          ),
        ),
      );
      tester.render();
      var finished = false, typed = 0;
      final ready = queue.decode(Uint8List.fromList(png), token).then((value) {
        finished = true;
        return value;
      });
      while (!finished) {
        await Future<void>.delayed(const Duration(milliseconds: 5));
        if (finished) break;
        tester.type('x');
        final frame = tester.render();
        typed++;
        expect(editor.source, 'draft${'x' * typed}');
        final caret = focus.caretRect!;
        expect(frame.atColRow(caret.left - 1, caret.top).grapheme, 'x');
      }
      final prepared = await ready;
      expect(
        typed,
        greaterThan(0),
        reason: 'UI input must run while decode is active',
      );
      expect((prepared.image.width, prepared.image.height), (640, 640));
      expect(prepared.png, isNotEmpty);
    },
    timeout: const Timeout(Duration(seconds: 20)),
  );
}
