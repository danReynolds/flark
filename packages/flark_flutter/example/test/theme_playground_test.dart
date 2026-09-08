import 'dart:io';
import 'package:flex_color_picker/flex_color_picker.dart';
import 'package:flark_dogfood/theme_playground.dart';
import 'package:flark_dogfood/theme_settings.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  final backend = createParseBackend();
  testWidgets(
    'playground changes appearance without replacing edits and exports controls',
    (t) async {
      String? copied;
      t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
        SystemChannels.platform,
        (call) async {
          if (call.method == 'Clipboard.setData') {
            copied = (call.arguments as Map)['text'] as String;
          }
          return null;
        },
      );
      addTearDown(
        () => t.binding.defaultBinaryMessenger.setMockMethodCallHandler(
          SystemChannels.platform,
          null,
        ),
      );
      await t.pumpWidget(MaterialApp(home: ThemePlayground(backend: backend)));
      await t.pump();
      expect(find.byType(FlarkEditorWidget), findsOneWidget);
      final editor = t
          .widget<FlarkEditorWidget>(
            find.byKey(const ValueKey('playground-editor')),
          )
          .controller;
      editor.command(SetSelection.caret(editor.text.length));
      editor.command(const InsertText(' Keep this edit.'));
      await t.pump();
      final source = editor.text, selection = editor.value.selection;
      await t.tap(find.text('Dark'));
      await t.pump();
      expect(editor.text, source);
      expect(editor.value.selection, selection);
      final context = t.element(
        find.byKey(const ValueKey('playground-editor')),
      );
      expect(Theme.of(context).brightness, Brightness.dark);
      await t.tap(find.text('Notebook'));
      await t.pump();
      expect(
        t
            .widget<FlarkEditorWidget>(
              find.byKey(const ValueKey('playground-editor')),
            )
            .linkPopoverBuilder,
        isNotNull,
      );
      final link = editor.editor.document.resources.firstWhere(
        (r) => !r.isImage,
      );
      editor.command(SetSelection.caret(link.contentStart + 1));
      await t.pump();
      await t.tap(find.byTooltip('Link'));
      await t.pumpAndSettle();
      expect(find.text('Link details'), findsOneWidget);
      await t.enterText(
        find.widgetWithText(TextField, 'URL'),
        'https://dart.dev/',
      );
      await t.tap(find.text('Apply changes'));
      await t.pump();
      expect(
        editor.editor.document.resources.first.destination,
        'https://dart.dev/',
      );
      await t.pumpAndSettle();
      await t.tap(find.byTooltip('Copy Dart configuration'));
      await t.pumpAndSettle();
      expect(copied, isNotNull);
      expect(copied, contains('FlarkThemeData('));
      expect(copied, contains('Widget brandedLinkPopover('));
      expect(copied, contains('Future<void> showResourceSheet('));
      expect(copied, isNot(contains("package:flark_flutter/src/")));
      final edited = editor.text;
      await t.tap(find.byTooltip('Reset theme'));
      await t.pump();
      expect(editor.text, edited);
      expect(editor.editor.history.canUndo, isTrue);
      await t.tap(find.byTooltip('Reset sample'));
      await t.pump();
      expect(editor.text, themeSample);
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  testWidgets('narrow playground keeps controls and a single editor usable', (
    t,
  ) async {
    t.view.physicalSize = const Size(460, 780);
    t.view.devicePixelRatio = 1;
    addTearDown(t.view.resetPhysicalSize);
    addTearDown(t.view.resetDevicePixelRatio);
    await t.pumpWidget(MaterialApp(home: ThemePlayground(backend: backend)));
    await t.pump();
    expect(find.text('Customize'), findsOneWidget);
    expect(find.text('Live editor'), findsOneWidget);
    expect(find.text('Read-only preview'), findsNothing);
    expect(find.byType(FlarkEditorWidget), findsOneWidget);
    expect(find.byTooltip('Link'), findsOneWidget);
    expect(t.takeException(), isNull);
    await t.pumpAndSettle();
    await t.pumpWidget(const SizedBox());
  });

  testWidgets(
    'visual color and hex changes keep writing and theme fields in sync',
    (t) async {
      t.view.physicalSize = const Size(1100, 900);
      t.view.devicePixelRatio = 1;
      addTearDown(t.view.resetPhysicalSize);
      addTearDown(t.view.resetDevicePixelRatio);
      await t.pumpWidget(MaterialApp(home: ThemePlayground(backend: backend)));
      await t.pump();
      final host = find.byKey(const ValueKey('playground-editor'));
      final editor = t.widget<FlarkEditorWidget>(host).controller;
      editor.command(SetSelection.caret(editor.text.length));
      editor.command(const InsertText(' Still writing.'));
      await t.pump();
      final source = editor.text, caret = editor.value.selection;
      Color linkColor() => FlarkThemeData.resolve(
        t.element(host),
        overrides: t.widget<FlarkEditorWidget>(host).theme,
      ).styles[FlarkTextRole.link]!.color!;
      final before = linkColor();
      await t.tap(find.byTooltip('Choose Link color'));
      await t.pump();
      final wheel = find.byType(ColorWheelPicker);
      await t.ensureVisible(wheel);
      await t.pump();
      final rect = t.getRect(wheel);
      await t.tapAt(Offset(rect.center.dx, rect.top + 10));
      await t.pump();
      expect(linkColor(), isNot(before));
      final hexField = find.widgetWithText(TextField, 'Link color');
      expect(
        t.widget<TextField>(hexField).controller!.text,
        linkColor().toARGB32().toRadixString(16).substring(2).toUpperCase(),
      );
      await t.enterText(hexField, '804080C0');
      await t.pump();
      expect(linkColor(), const Color(0x804080c0));
      expect(
        t.widget<ColorPicker>(find.byType(ColorPicker)).color,
        const Color(0x804080c0),
      );
      await t.enterText(hexField, 'invalid');
      await t.pump();
      expect(linkColor(), const Color(0x804080c0));
      expect(editor.text, source);
      expect(editor.value.selection, caret);
      await t.ensureVisible(find.byTooltip('Close Link color'));
      await t.tap(find.byTooltip('Close Link color'));
      await t.pump();
      await t.ensureVisible(find.text('Dark'));
      await t.tap(find.text('Dark'));
      await t.pump();
      expect(
        t.widget<TextField>(hexField).controller!.text,
        linkColor().toARGB32().toRadixString(16).substring(2).toUpperCase(),
      );
      editor.command(const Undo());
      await t.pump();
      expect(editor.text, themeSample);
      await t.pumpAndSettle();
      await t.pumpWidget(const SizedBox());
    },
  );

  test(
    'write exported public consumers for the separate compile/run check',
    () {
      final controls = File('lib/custom_controls.dart').readAsStringSync();
      for (final local in [false, true]) {
        final settings = ThemeSettings()..preset('Notebook');
        settings.perInstance = local;
        settings.brightness = Brightness.dark;
        settings.styles[FlarkTextRole.link] = const TextStyle(
          color: Color(0xffabcdef),
        );
        settings.metrics[FlarkMetric.rowSpacing] = 17;
        settings.syntaxColors[FlarkSyntaxRole.string] = const Color(0xfffedcba);
        File(
          '.dart_tool/flark_theme_${local ? 'instance' : 'ambient'}.dart',
        ).writeAsStringSync(
          settings.exportDart(customControlsSource: controls),
        );
      }
      File('.dart_tool/flark_theme_export_test.dart').writeAsStringSync('''
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark_flutter/flark_flutter.dart';
import 'flark_theme_ambient.dart' as ambient;
import 'flark_theme_instance.dart' as instance;
void main() {
  for (final local in [false, true]) {
    testWidgets('exported configuration local=\$local', (t) async {
      final backend = createParseBackend();
      final editor = FlarkController(FlarkEditor(backend, text: '[link](/guide)'));
      await t.pumpWidget(MaterialApp(home: Scaffold(body: local
        ? instance.ConfiguredMarkdown(editor: editor)
        : ambient.ConfiguredMarkdown(editor: editor))));
      final host = find.byType(FlarkEditorWidget).first;
      final widget = t.widget<FlarkEditorWidget>(host);
      final theme = FlarkThemeData.resolve(t.element(host), overrides: widget.theme);
      expect(theme.styles[FlarkTextRole.link]!.color, const Color(0xffabcdef));
      expect(theme.styles[FlarkTextRole.body]!.fontFamily, 'Georgia');
      expect(theme.metrics[FlarkMetric.rowSpacing], 17);
      expect(theme.syntaxColors[FlarkSyntaxRole.string], const Color(0xfffedcba));
      expect(widget.linkPopoverBuilder, isNotNull);
      expect(widget.presentResourceEditor, isNotNull);
      expect(Theme.of(t.element(host)).brightness, Brightness.dark);
      await t.pumpWidget(const SizedBox());
      editor.dispose();
    });
  }
}
''');
    },
  );
}
