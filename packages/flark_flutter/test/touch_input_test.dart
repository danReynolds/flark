import 'package:flark_flutter/flark_flutter_legacy.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  final paragraphs = List.generate(
    60,
    (i) => 'Paragraph $i has a few words.',
  ).join('\n\n');

  Future<(FlarkController, RenderFlarkSurface, ScrollPosition)> pumpEditor(
    WidgetTester tester, {
    String? text,
    int caret = 0,
    bool readOnly = false,
    ValueChanged<Uri>? onOpenLink,
    DeviceGestureSettings? gestureSettings,
  }) async {
    final c = FlarkController(
      FlarkEditor(backend, text: text ?? paragraphs, caret: caret),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: SizedBox(
            height: 400,
            child: Builder(
              builder: (context) => MediaQuery(
                data: MediaQuery.of(context).copyWith(
                  gestureSettings:
                      gestureSettings ?? MediaQuery.of(context).gestureSettings,
                ),
                child: FlarkEditorWidget(
                  controller: c,
                  readOnly: readOnly,
                  showToolbar: false,
                  onOpenLink: onOpenLink,
                ),
              ),
            ),
          ),
        ),
      ),
    );
    await tester.pump();
    final surface = tester.renderObject<RenderFlarkSurface>(
      find.byType(FlarkSurface),
    );
    final scrollable = tester.state<ScrollableState>(
      find.ancestor(
        of: find.byType(FlarkSurface),
        matching: find.byType(Scrollable),
      ),
    );
    return (c, surface, scrollable.position);
  }

  /// Records clipboard writes, serves [paste] to reads, and absorbs haptics.
  List<String> mockPlatform(WidgetTester tester, {String paste = ''}) {
    final copied = <String>[];
    final messenger = tester.binding.defaultBinaryMessenger;
    messenger.setMockMethodCallHandler(SystemChannels.platform, (call) async {
      switch (call.method) {
        case 'Clipboard.setData':
          copied.add((call.arguments as Map)['text'] as String);
        case 'Clipboard.getData':
          return {'text': paste};
        case 'Clipboard.hasStrings':
          return {'value': paste.isNotEmpty};
      }
      return null;
    });
    addTearDown(
      () => messenger.setMockMethodCallHandler(SystemChannels.platform, null),
    );
    return copied;
  }

  const menu = ['Cut', 'Copy', 'Paste', 'Select all'];

  Future<void> dispose(WidgetTester tester, FlarkController c) async {
    await tester.pump(const Duration(milliseconds: 350));
    await tester.pumpWidget(const SizedBox());
    c.dispose();
  }

  testWidgets('a touch scroll moves neither the caret nor the keyboard', (
    tester,
  ) async {
    final (c, surface, position) = await pumpEditor(tester);
    final before = c.editor.selection;
    await tester.dragFrom(
      surface.localToGlobal(const Offset(120, 200)),
      const Offset(0, -240),
    );
    await tester.pump();
    expect(position.pixels, greaterThan(0));
    expect(c.editor.selection, before);
    expect(tester.testTextInput.hasAnyClients, isFalse);
    expect(tester.testTextInput.isVisible, isFalse);
    await dispose(tester, c);
  });

  testWidgets('a touch places the caret and raises the keyboard as it lifts', (
    tester,
  ) async {
    final (c, surface, _) = await pumpEditor(tester);
    final lineEnd = surface.localToGlobal(
      Offset(700, surface.caretRect.center.dy),
    );
    final gesture = await tester.startGesture(lineEnd);
    await tester.pump();
    expect(c.editor.selection, const FlarkSelection.collapsed(0));
    expect(tester.testTextInput.hasAnyClients, isFalse);
    await gesture.up();
    await tester.pump();
    expect(
      c.editor.selection,
      FlarkSelection.collapsed('Paragraph 0 has a few words.'.length),
    );
    expect(tester.testTextInput.isVisible, isTrue);
    await dispose(tester, c);
  });

  testWidgets('a tap raises a keyboard the platform hid', (tester) async {
    final (c, surface, _) = await pumpEditor(tester);
    final point = surface.localToGlobal(surface.caretRect.center);
    await tester.tapAt(point);
    await tester.pump();
    expect(tester.testTextInput.isVisible, isTrue);
    // Android's back gesture hides the keyboard and keeps the connection.
    await SystemChannels.textInput.invokeMethod<void>('TextInput.hide');
    expect(tester.testTextInput.isVisible, isFalse);
    await tester.pump(kDoubleTapTimeout);
    await tester.tapAt(point);
    await tester.pump();
    expect(tester.testTextInput.isVisible, isTrue);
    await dispose(tester, c);
  });

  testWidgets('the touch that stops a fling does not place the caret', (
    tester,
  ) async {
    final (c, surface, position) = await pumpEditor(tester);
    final before = c.editor.selection;
    await tester.flingFrom(
      surface.localToGlobal(const Offset(120, 300)),
      const Offset(0, -200),
      2000,
    );
    await tester.pump();
    await tester.pump(const Duration(milliseconds: 40));
    expect(position.isScrollingNotifier.value, isTrue);
    await tester.tapAt(tester.getCenter(find.byType(FlarkEditorWidget)));
    await tester.pump();
    expect(position.isScrollingNotifier.value, isFalse);
    expect(c.editor.selection, before);
    expect(tester.testTextInput.hasAnyClients, isFalse);
    await dispose(tester, c);
  });

  testWidgets('a touch double tap selects the word it lands on', (
    tester,
  ) async {
    final (c, surface, _) = await pumpEditor(
      tester,
      text: 'A simple word.',
      caret: 4,
    );
    final point = surface.localToGlobal(surface.caretRect.center);
    await tester.tapAt(point);
    await tester.pump(kDoubleTapMinTime);
    await tester.tapAt(point);
    await tester.pump();
    expect(c.editor.selection, const FlarkSelection(2, 8));
    for (final label in menu) {
      expect(find.text(label), findsOneWidget);
    }
    await dispose(tester, c);
  });

  testWidgets('the reader opens a link on a tap, not on a scroll', (
    tester,
  ) async {
    final opened = <Uri>[];
    final (c, surface, position) = await pumpEditor(
      tester,
      text: '[guide](https://example.com/guide)\n\n$paragraphs',
      readOnly: true,
      onOpenLink: opened.add,
    );
    final link = surface.localToGlobal(
      surface.caretRect.center + const Offset(12, 0),
    );
    await tester.dragFrom(link, const Offset(0, -240));
    await tester.pumpAndSettle();
    expect(opened, isEmpty);
    position.jumpTo(0);
    await tester.pump();
    await tester.tapAt(link);
    expect(opened, [Uri.parse('https://example.com/guide')]);
    await dispose(tester, c);
  });

  testWidgets('a drag past the platform slop scrolls and is not a tap', (
    tester,
  ) async {
    // Android's scroll view starts at its own slop, below Flutter's default.
    final (c, surface, position) = await pumpEditor(
      tester,
      gestureSettings: const DeviceGestureSettings(touchSlop: 8),
    );
    final before = c.editor.selection;
    final gesture = await tester.startGesture(
      surface.localToGlobal(const Offset(120, 200)),
    );
    for (var i = 0; i < 3; i++) {
      await gesture.moveBy(const Offset(0, -5));
      await tester.pump();
    }
    expect(position.pixels, greaterThan(0));
    await gesture.up();
    await tester.pump();
    expect(c.editor.selection, before);
    expect(tester.testTextInput.hasAnyClients, isFalse);
    await dispose(tester, c);
  });

  for (final (label, moves, scrolls) in [
    ('a tap with a little jitter opens the link', [1.0, -1.0], false),
    (
      'a drag past the slop scrolls and opens nothing',
      [-4.0, -4.0, -4.0, -4.0],
      true,
    ),
  ]) {
    testWidgets('in the reader, $label', (tester) async {
      final opened = <Uri>[];
      // Android's slop, where the reader's scroll physics has no start
      // distance: only the touch slop keeps a jittering tap a tap.
      final (c, surface, position) = await pumpEditor(
        tester,
        text: '[guide](https://example.com/guide)\n\n$paragraphs',
        readOnly: true,
        onOpenLink: opened.add,
        gestureSettings: const DeviceGestureSettings(touchSlop: 8),
      );
      final gesture = await tester.startGesture(
        surface.localToGlobal(surface.caretRect.center + const Offset(12, 0)),
      );
      for (final dy in moves) {
        await gesture.moveBy(Offset(0, dy));
        await tester.pump();
      }
      expect(position.pixels, scrolls ? greaterThan(0) : 0);
      await gesture.up();
      await tester.pumpAndSettle();
      expect(
        opened,
        scrolls ? isEmpty : [Uri.parse('https://example.com/guide')],
      );
      await dispose(tester, c);
    });
  }

  group('touch selection', () {
    const text = 'A simple word here.';

    Future<(FlarkController, RenderFlarkSurface)> longPressWord(
      WidgetTester tester,
    ) async {
      final (c, surface, _) = await pumpEditor(tester, text: text);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(5).center),
      );
      await tester.pump();
      return (c, surface);
    }

    /// Drags a handle from [from] through [path]: past the pan slop to start
    /// the drag, then to each point.
    Future<void> dragHandle(
      WidgetTester tester,
      Offset from,
      List<Offset> path, {
      void Function()? between,
    }) async {
      final gesture = await tester.startGesture(from);
      await gesture.moveBy(Offset((path.first - from).dx.sign * 40, 0));
      await tester.pump();
      for (final (i, point) in path.indexed) {
        await gesture.moveTo(point);
        await tester.pump();
        if (i == 0) between?.call();
      }
      await gesture.up();
      await tester.pump();
    }

    testWidgets('a long press selects a word and shows the menu', (
      tester,
    ) async {
      mockPlatform(tester);
      final (c, _) = await longPressWord(tester);
      expect(c.editor.selection, const FlarkSelection(2, 8));
      for (final label in menu) {
        expect(find.text(label), findsOneWidget);
      }
      expect(tester.testTextInput.isVisible, isTrue);
      await dispose(tester, c);
    });

    testWidgets('dragging a long press extends it by words', (tester) async {
      mockPlatform(tester);
      final (c, surface, _) = await pumpEditor(tester, text: text);
      final gesture = await tester.startGesture(
        surface.localToGlobal(surface.caretRectAt(5).center),
      );
      await tester.pump(kLongPressTimeout + kPressTimeout);
      expect(c.editor.selection, const FlarkSelection(2, 8));
      await gesture.moveTo(
        surface.localToGlobal(surface.caretRectAt(15).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(2, 18));
      expect(find.text('Copy'), findsNothing);
      await gesture.up();
      await tester.pump();
      expect(find.text('Copy'), findsOneWidget);
      await dispose(tester, c);
    });

    testWidgets('Copy copies the word and collapses to its end', (
      tester,
    ) async {
      final copied = mockPlatform(tester);
      final (c, _) = await longPressWord(tester);
      await tester.tap(find.text('Copy'));
      await tester.pump();
      expect(copied, ['simple']);
      expect(c.editor.selection, const FlarkSelection.collapsed(8));
      expect(find.text('Copy'), findsNothing);
      await dispose(tester, c);
    });

    testWidgets('Cut and Paste edit through the kernel', (tester) async {
      final copied = mockPlatform(tester, paste: 'plain');
      final (c, surface) = await longPressWord(tester);
      await tester.tap(find.text('Cut'));
      await tester.pump();
      expect(copied, ['simple']);
      expect(c.text, 'A  word here.');
      expect(find.text('Paste'), findsNothing);
      await tester.pump(kDoubleTapTimeout);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(5).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(3, 7));
      await tester.tap(find.text('Paste'));
      await tester.pump();
      expect(c.text, 'A  plain here.');
      expect(c.editor.selection, const FlarkSelection.collapsed(8));
      await dispose(tester, c);
    });

    testWidgets('the end handle moves only the end', (tester) async {
      mockPlatform(tester);
      final (c, surface) = await longPressWord(tester);
      final end = surface.caretRectAt(8).bottomLeft;
      final from = surface.localToGlobal(end + const Offset(11, 11));
      final to = surface.localToGlobal(
        Offset(surface.caretRectAt(18).left + 1, end.dy + 11),
      );
      await dragHandle(tester, from, [to]);
      expect(c.editor.selection, const FlarkSelection(2, 18));
      // The menu returns when the handle is let go.
      expect(find.text('Copy'), findsOneWidget);
      await dispose(tester, c);
    });

    testWidgets('the start handle stops before the end', (tester) async {
      mockPlatform(tester);
      final (c, surface) = await longPressWord(tester);
      final start = surface.caretRectAt(2).bottomLeft;
      final from = surface.localToGlobal(start + const Offset(-11, 11));
      await dragHandle(
        tester,
        from,
        [
          surface.localToGlobal(
            Offset(surface.caretRectAt(5).left + 1, start.dy + 11),
          ),
          surface.localToGlobal(Offset(700, start.dy + 11)),
        ],
        between: () => expect(c.editor.selection, const FlarkSelection(8, 5)),
      );
      expect(c.editor.selection, const FlarkSelection(8, 5));
      await dispose(tester, c);
    });

    testWidgets('typing ends the touch selection', (tester) async {
      mockPlatform(tester);
      final (c, _) = await longPressWord(tester);
      tester.testTextInput.updateEditingValue(
        const TextEditingValue(
          text: 'A x word here.',
          selection: TextSelection.collapsed(offset: 3),
        ),
      );
      await tester.pump();
      expect(c.text, 'A x word here.');
      expect(find.text('Copy'), findsNothing);
      await dispose(tester, c);
    });

    testWidgets('a style keeps the handles and hides the menu', (tester) async {
      mockPlatform(tester);
      final (c, surface) = await longPressWord(tester);
      expect(c.command(const ToggleStyle(Style.strong)), isTrue);
      await tester.pump();
      expect(c.text, 'A **simple** word here.');
      expect(c.editor.selection.isCollapsed, isFalse);
      expect(surface.handles, isNotNull);
      expect(find.text('Copy'), findsNothing);
      await dispose(tester, c);
    });

    testWidgets('a tap ends the touch selection', (tester) async {
      mockPlatform(tester);
      final (c, surface) = await longPressWord(tester);
      expect(surface.handles, isNotNull);
      await tester.pump(kDoubleTapTimeout);
      await tester.tapAt(surface.localToGlobal(surface.caretRectAt(15).center));
      await tester.pump();
      expect(c.editor.selection.isCollapsed, isTrue);
      expect(find.text('Copy'), findsNothing);
      expect(surface.handles, isNull);
      await dispose(tester, c);
    });

    testWidgets('a mouse held still does not open the menu', (tester) async {
      mockPlatform(tester);
      final (c, surface, _) = await pumpEditor(tester, text: text);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(5).center),
        kind: PointerDeviceKind.mouse,
      );
      await tester.pump();
      expect(c.editor.selection.isCollapsed, isTrue);
      expect(find.text('Paste'), findsNothing);
      await dispose(tester, c);
    });

    testWidgets('a long press on a blank line offers Paste and Select all', (
      tester,
    ) async {
      mockPlatform(tester);
      const source = 'First\n\nSecond';
      final (c, surface, _) = await pumpEditor(tester, text: source);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(6).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection.collapsed(6));
      expect(find.text('Copy'), findsNothing);
      expect(find.text('Paste'), findsOneWidget);
      await tester.tap(find.text('Select all'));
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(0, source.length));
      for (final label in menu) {
        expect(find.text(label), findsOneWidget);
      }
      await dispose(tester, c);
    });
    testWidgets('a long press on a blank line after a selection offers Paste', (
      tester,
    ) async {
      mockPlatform(tester);
      final (c, surface, _) = await pumpEditor(
        tester,
        text: 'First word\n\nSecond',
      );
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(8).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(6, 10));
      await tester.pump(kDoubleTapTimeout);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(11).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection.collapsed(11));
      expect(find.text('Paste'), findsOneWidget);
      expect(find.byType(TextMagnifier), findsNothing);
      await dispose(tester, c);
    });

    testWidgets('a long press dragged away and back keeps its gesture', (
      tester,
    ) async {
      mockPlatform(tester);
      final (c, surface, _) = await pumpEditor(
        tester,
        text: 'First word\n\nSecond',
      );
      final blank = surface.localToGlobal(surface.caretRectAt(11).center);
      final gesture = await tester.startGesture(blank);
      await tester.pump(kLongPressTimeout + kPressTimeout);
      await gesture.moveTo(
        surface.localToGlobal(surface.caretRectAt(15).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(11, 18));
      await gesture.moveTo(blank);
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection.collapsed(11));
      await gesture.moveTo(
        surface.localToGlobal(surface.caretRectAt(15).center),
      );
      await tester.pump();
      expect(c.editor.selection, const FlarkSelection(11, 18));
      await gesture.up();
      await tester.pump();
      expect(find.text('Copy'), findsOneWidget);
      expect(find.byType(TextMagnifier), findsNothing);
      await dispose(tester, c);
    });

    testWidgets(
      'a cancelled long press keeps its selection, not the magnifier',
      (tester) async {
        mockPlatform(tester);
        final (c, surface, _) = await pumpEditor(tester, text: text);
        final gesture = await tester.startGesture(
          surface.localToGlobal(surface.caretRectAt(5).center),
        );
        await tester.pump(kLongPressTimeout + kPressTimeout);
        expect(find.byType(TextMagnifier), findsOneWidget);
        await gesture.cancel();
        await tester.pump();
        expect(find.byType(TextMagnifier), findsNothing);
        expect(c.editor.selection, const FlarkSelection(2, 8));
        expect(surface.handles, isNotNull);
        await dispose(tester, c);
      },
    );

    testWidgets('the menu returns when a scroll brings its selection back', (
      tester,
    ) async {
      mockPlatform(tester);
      final (c, surface, position) = await pumpEditor(tester);
      await tester.longPressAt(
        surface.localToGlobal(surface.caretRectAt(3).center),
      );
      await tester.pump();
      expect(find.text('Copy'), findsOneWidget);
      await tester.dragFrom(
        surface.localToGlobal(const Offset(120, 300)),
        const Offset(0, -300),
      );
      await tester.pumpAndSettle();
      expect(position.pixels, greaterThan(100));
      expect(find.text('Copy'), findsNothing);
      position.jumpTo(0);
      await tester.pump();
      expect(find.text('Copy'), findsOneWidget);
      await dispose(tester, c);
    });
  });
}
