import 'dart:async';
import 'dart:ui' show PointerDeviceKind;
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flark_flutter/src/surface.dart';
import 'package:flutter/material.dart';
import 'package:flutter/gestures.dart' show PointerScrollEvent;
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  FlarkController controller(String text, [int caret = 0]) =>
      FlarkController(FlarkEditor(backend, text: text, caret: caret));
  RenderFlarkSurface surface(WidgetTester t) =>
      t.renderObject(find.byType(FlarkSurface).first);
  Future<void> close(WidgetTester t, List<FlarkController> cs) async {
    await t.pump(const Duration(milliseconds: 350));
    await t.pumpWidget(const SizedBox());
    for (final c in cs) {
      c.dispose();
    }
  }

  testWidgets(
    'ambient and instance themes compose identical viewer/editor styles',
    (t) async {
      const text = '[**~~linked~~**](/guide)';
      final editor = controller(text, 8), viewer = controller(text);
      final edited = <FlarkPaintObservation>[],
          viewed = <FlarkPaintObservation>[];
      final local = FlarkThemeData(
        styles: {FlarkTextRole.link: const TextStyle(color: Colors.orange)},
      );
      await t.pumpWidget(
        MaterialApp(
          theme: ThemeData(
            extensions: [
              FlarkThemeData(
                styles: {
                  FlarkTextRole.body: const TextStyle(fontSize: 24),
                  FlarkTextRole.link: const TextStyle(letterSpacing: 1),
                },
              ),
            ],
          ),
          home: Row(
            children: [
              Expanded(
                child: FlarkEditorWidget(
                  controller: editor,
                  showToolbar: false,
                  theme: local,
                  onPaint: edited.add,
                ),
              ),
              Expanded(
                child: FlarkMarkdownView(
                  controller: viewer,
                  theme: local,
                  onPaint: viewed.add,
                ),
              ),
            ],
          ),
        ),
      );
      final style = edited.first.resolvedStyles.single.single;
      expect(style.color, Colors.orange);
      expect(style.fontSize, 24);
      expect(style.letterSpacing, 1);
      expect(style.fontWeight, FontWeight.w700);
      expect(style.decoration!.contains(TextDecoration.underline), isTrue);
      expect(style.decoration!.contains(TextDecoration.lineThrough), isTrue);
      expect(viewed.first.resolvedStyles, edited.first.resolvedStyles);
      expect(editor.editor.history.canUndo, isFalse);
      await close(t, [editor, viewer]);
    },
  );

  testWidgets(
    'theme changes retain images, source, selection and first-frame hit geometry',
    (t) async {
      final c = controller('[hello](/guide)\n\n![image](/image)\n\nlast', 4);
      var loads = 0;
      ImageProvider<Object>? provider(Uri _) {
        loads++;
        return null;
      }

      final paints = <FlarkPaintObservation>[];
      Widget app(Brightness brightness, double size, double scale) =>
          MaterialApp(
            themeAnimationDuration: Duration.zero,
            theme: ThemeData(brightness: brightness),
            home: MediaQuery(
              data: MediaQueryData(textScaler: TextScaler.linear(scale)),
              child: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                showToolbar: false,
                imageProvider: provider,
                theme: FlarkThemeData(
                  styles: {FlarkTextRole.body: TextStyle(fontSize: size)},
                ),
                onPaint: paints.add,
              ),
            ),
          );
      await t.pumpWidget(app(Brightness.light, 16, 1));
      await t.pump();
      final before = paints.last;
      final source = c.text, selection = c.value.selection;
      final count = loads;
      paints.clear();
      await t.pumpWidget(app(Brightness.dark, 16, 1));
      expect(paints.first.caret, before.caret);
      expect(
        paints.first.resolvedStyles.first.first.color,
        isNot(before.resolvedStyles.first.first.color),
      );
      expect(loads, count);
      paints.clear();
      await t.pumpWidget(app(Brightness.dark, 22, 1.3));
      expect(paints.first.caret!.height, greaterThan(before.caret!.height));
      final caret = paints.first.caret!;
      expect(surface(t).sourceAt(caret.center), selection.extentOffset);
      expect(c.text, source);
      expect(c.value.selection, selection);
      expect(c.editor.history.canUndo, isFalse);
      expect(loads, count);
      await close(t, [c]);
    },
  );

  for (final custom in [false, true]) {
    testWidgets(
      'click, ${custom ? 'custom' : 'default'} controls, edit and next input',
      (t) async {
        final c = controller('[hello](/old)', 3);
        final paints = <FlarkPaintObservation>[];
        await t.pumpWidget(
          MaterialApp(
            home: Scaffold(
              body: FlarkEditorWidget(
                controller: c,
                autofocus: true,
                onPaint: paints.add,
                linkPopoverBuilder: custom
                    ? (_, a) => Material(
                        child: TextButton(
                          onPressed: a.edit,
                          child: const Text('Change destination'),
                        ),
                      )
                    : null,
                presentResourceEditor: custom
                    ? (context, session) => showDialog<void>(
                        context: context,
                        builder: (context) => AlertDialog(
                          actions: [
                            TextButton(
                              onPressed: () {
                                if (session.save(
                                  destination: '/new',
                                  label: session.label,
                                )) {
                                  Navigator.pop(context);
                                }
                              },
                              child: const Text('Apply custom edit'),
                            ),
                          ],
                        ),
                      )
                    : null,
              ),
            ),
          ),
        );
        await t.pump();
        await t.tapAt(surface(t).localToGlobal(surface(t).caretRect.center));
        await t.pump();
        expect(c.editor.selection.isCollapsed, isTrue);
        expect(c.text, '[hello](/old)');
        await t.tap(find.text(custom ? 'Change destination' : 'Edit'));
        await t.pumpAndSettle(); // Opening the dialog, before the proving edit.
        if (!custom) {
          await t.enterText(
            find.widgetWithText(TextField, 'Destination'),
            '/new',
          );
        }
        paints.clear();
        await t.tap(find.text(custom ? 'Apply custom edit' : 'Save'));
        await t.pump();
        expect(c.text, '[hello](</new>)');
        expect(paints.first.snapshot.source, c.text);
        await t.pumpAndSettle(); // Close animation before actual text input.
        t.testTextInput.updateEditingValue(
          c.value.copyWith(
            text:
                '${c.text.substring(0, c.editor.selection.extent)}!${c.text.substring(c.editor.selection.extent)}',
            selection: TextSelection.collapsed(
              offset: c.editor.selection.extent + 1,
            ),
          ),
        );
        await t.pump();
        expect(c.text, '[hello!](</new>)');
        c.command(const Undo());
        await t.pump();
        expect(c.text, '[hello](</new>)');
        c.command(const Undo());
        await t.pump();
        expect(c.text, '[hello](/old)');
        await close(t, [c]);
      },
    );
  }

  testWidgets(
    'old popover and presenter actions cannot edit a changed selection',
    (t) async {
      final c = controller('[hello](/old) tail', 3);
      FlarkLinkActions? actions;
      FlarkResourceSession? session;
      final done = Completer<void>();
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              linkPopoverBuilder: (_, a) {
                actions = a;
                return FlarkLinkPopover(actions: a);
              },
              presentResourceEditor: (_, s) {
                session = s;
                return done.future;
              },
            ),
          ),
        ),
      );
      await t.pump();
      await t.tapAt(surface(t).localToGlobal(surface(t).caretRect.center));
      await t.pump();
      final old = actions!;
      c.command(const SetSelection.caret(15));
      old.remove!();
      await t.pump();
      expect(c.text, '[hello](/old) tail');
      expect(find.byType(FlarkLinkPopover), findsNothing);
      c.command(const SetSelection.caret(3));
      await t.pump();
      await t.tap(find.byTooltip('Link'));
      expect(session!.active, isTrue);
      c.command(const SetSelection.caret(15));
      expect(session!.save(destination: '/new', label: 'hello'), isFalse);
      expect(session!.remove(), isFalse);
      expect(c.text, '[hello](/old) tail');
      done.complete();
      await t.pump();
      expect(session!.active, isFalse);
      await close(t, [c]);
    },
  );

  testWidgets(
    'drag never opens actions; keyboard actions and Escape retain caret',
    (t) async {
      final c = controller('[a long link label](/guide)\n\nfollowing', 4);
      await t.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: FlarkEditorWidget(
              controller: c,
              autofocus: true,
              showToolbar: false,
            ),
          ),
        ),
      );
      await t.pump();
      final point = surface(t).localToGlobal(surface(t).caretRect.center);
      await t.dragFrom(
        point,
        const Offset(75, 0),
        kind: PointerDeviceKind.mouse,
      );
      await t.pump();
      expect(find.byType(FlarkLinkPopover), findsNothing);
      expect(c.editor.selection.isCollapsed, isFalse);
      c.command(const SetSelection.caret(4));
      await t.pump();
      await t.sendKeyDownEvent(LogicalKeyboardKey.shiftLeft);
      await t.sendKeyEvent(LogicalKeyboardKey.f10);
      await t.sendKeyUpEvent(LogicalKeyboardKey.shiftLeft);
      await t.pump();
      await t.pump();
      expect(find.byType(FlarkLinkPopover), findsOneWidget);
      await t.sendKeyEvent(LogicalKeyboardKey.escape);
      await t.pump();
      expect(find.byType(FlarkLinkPopover), findsNothing);
      expect(c.editor.selection.extent, 4);
      expect(t.testTextInput.isRegistered, isTrue);
      await close(t, [c]);
    },
  );

  testWidgets('built-in text palettes have contrast on their actual surfaces', (
    t,
  ) async {
    double contrast(Color a, Color b) {
      final x = a.computeLuminance(), y = b.computeLuminance();
      return x > y ? (x + .05) / (y + .05) : (y + .05) / (x + .05);
    }

    for (final brightness in Brightness.values) {
      late FlarkThemeData theme;
      await t.pumpWidget(
        MaterialApp(
          theme: ThemeData(brightness: brightness),
          home: Builder(
            builder: (context) {
              theme = FlarkThemeData.resolve(context);
              return const SizedBox();
            },
          ),
        ),
      );
      final canvas = theme.colors[FlarkColorRole.canvas]!;
      expect(
        contrast(theme.styles[FlarkTextRole.body]!.color!, canvas),
        greaterThanOrEqualTo(4.5),
      );
      expect(
        contrast(theme.styles[FlarkTextRole.link]!.color!, canvas),
        greaterThanOrEqualTo(4.5),
      );
      for (final e in theme.syntaxColors.entries) {
        expect(
          contrast(e.value, theme.colors[FlarkColorRole.codeBackground]!),
          greaterThanOrEqualTo(4.5),
          reason: '$brightness ${e.key}',
        );
      }
    }
  });

  testWidgets('a color-only change preserves a manually scrolled viewport', (
    t,
  ) async {
    final c = controller(
      List.generate(45, (i) => 'paragraph $i').join('\n\n'),
      3,
    );
    final paints = <FlarkPaintObservation>[];
    Widget app(Color color) => MaterialApp(
      home: FlarkEditorWidget(
        controller: c,
        autofocus: true,
        showToolbar: false,
        theme: FlarkThemeData(colors: {FlarkColorRole.canvas: color}),
        onPaint: paints.add,
      ),
    );
    await t.pumpWidget(app(Colors.white));
    await t.pump();
    await t.sendEventToBinding(
      PointerScrollEvent(
        position: surface(t).localToGlobal(const Offset(100, 100)),
        scrollDelta: const Offset(0, 350),
      ),
    );
    await t.pump(const Duration(seconds: 1));
    final scrolled = surface(t).scrollOffset;
    expect(scrolled, greaterThan(0));
    paints.clear();
    await t.pumpWidget(app(const Color(0xfffff0e0)));
    expect(surface(t).scrollOffset, scrolled);
    expect(paints.first.caret, isNull);
    expect(c.editor.selection.extent, 3);
    await close(t, [c]);
  });

  test('overrides are immutable and reject invalid layout values', () {
    final colors = {FlarkColorRole.canvas: Colors.red};
    final theme = FlarkThemeData(colors: colors);
    colors[FlarkColorRole.canvas] = Colors.blue;
    expect(theme.colors[FlarkColorRole.canvas], Colors.red);
    expect(() => theme.colors.clear(), throwsUnsupportedError);
    for (final value in [-1.0, double.nan, double.infinity, 3000.0]) {
      expect(
        () => FlarkThemeData(metrics: {FlarkMetric.rowSpacing: value}),
        throwsArgumentError,
      );
    }
    final first = FlarkThemeData(metrics: {FlarkMetric.rowSpacing: 40});
    final second = FlarkThemeData(metrics: {FlarkMetric.rowSpacing: 0});
    expect(first.lerp(second, 1.2).metrics[FlarkMetric.rowSpacing], 0);
  });

  test(
    'saving unchanged reference details preserves spelling and undo history',
    () {
      const source = '[**guide**][site]\n\n[site]: /guide "Details"';
      final c = controller(source, 5);
      final resource = c.editor.document.resources.first;
      var calls = 0;
      final session = FlarkResourceSession(
        image: false,
        resource: resource,
        selectedText: '',
        isActive: () => true,
        apply: (command) {
          calls++;
          return c.command(command);
        },
      );
      expect(
        session.save(
          destination: session.destination,
          label: session.label,
          title: session.title,
        ),
        isTrue,
      );
      expect(c.text, source);
      expect(c.editor.history.canUndo, isFalse);
      expect(calls, 0);
      expect(session.active, isFalse);
      c.dispose();
    },
  );

  testWidgets('Cmd-K in a fence keeps the code input active without a dialog', (
    t,
  ) async {
    final c = controller('```dart\nvalue\n```', 12);
    await t.pumpWidget(
      MaterialApp(
        home: Scaffold(body: FlarkEditorWidget(controller: c, autofocus: true)),
      ),
    );
    await t.pump();
    await t.sendKeyDownEvent(LogicalKeyboardKey.metaLeft);
    await t.sendKeyEvent(LogicalKeyboardKey.keyK);
    await t.sendKeyUpEvent(LogicalKeyboardKey.metaLeft);
    await t.pump();
    expect(find.text('Insert link'), findsNothing);
    expect(t.testTextInput.isRegistered, isTrue);
    expect(c.text, '```dart\nvalue\n```');
    await close(t, [c]);
  });
}
