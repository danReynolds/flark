import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'checkbox hit target uses click cursor and restores text cursor',
    (tester) async {
      final c = FlarkController(
        FlarkEditor(backend, text: '- [ ] todo\n\ntext'),
      );
      Widget app({bool readOnly = false}) => MaterialApp(
        home: Scaffold(
          body: FlarkEditorWidget(
            controller: c,
            showToolbar: false,
            readOnly: readOnly,
          ),
        ),
      );
      await tester.pumpWidget(app());
      final origin = tester.getTopLeft(find.byType(FlarkSurface));
      final mouse = await tester.createGesture(kind: PointerDeviceKind.mouse);
      await mouse.addPointer(location: origin + const Offset(70, 27));
      await tester.pump();
      final tracker = RendererBinding.instance.mouseTracker;
      expect(tracker.debugDeviceActiveCursor(1), SystemMouseCursors.text);
      await mouse.moveTo(origin + const Offset(23, 27));
      await tester.pump();
      expect(tracker.debugDeviceActiveCursor(1), SystemMouseCursors.click);
      await mouse.down(origin + const Offset(23, 27));
      await mouse.up();
      await tester.pump();
      expect(c.text, '- [x] todo\n\ntext');
      expect(tracker.debugDeviceActiveCursor(1), SystemMouseCursors.click);
      c.command(const Undo());
      await tester.pump();
      expect(c.text, '- [ ] todo\n\ntext');
      await mouse.moveTo(origin + const Offset(70, 27));
      await tester.pump();
      expect(tracker.debugDeviceActiveCursor(1), SystemMouseCursors.text);
      await mouse.moveTo(origin + const Offset(23, 27));
      await tester.pumpWidget(app(readOnly: true));
      expect(tracker.debugDeviceActiveCursor(1), SystemMouseCursors.basic);
      await mouse.removePointer();
      await tester.pumpWidget(const SizedBox());
      await tester.pump(kDoubleTapMinTime);
      c.dispose();
    },
  );
}
