import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark.dart';

Future<FlarkFlutterController> _parsedController(String markdown) async {
  final controller = FlarkFlutterController.fromMarkdown(markdown);
  final result = await FlarkNativeComrakParseBackend.withNativeBridge().parse(
    FlarkMarkdownParseRequest(
      revision: 0,
      markdown: markdown,
      profile: FlarkMarkdownProfile.commonMarkGfm,
    ),
  );
  expect(controller.applyParseResult(result), isTrue);
  return controller;
}

Color _decorationColor(WidgetTester tester, Finder finder) {
  final box = tester.widget<DecoratedBox>(finder);
  return (box.decoration as BoxDecoration).color!;
}

Finder _previewCodeBlockDecoration() {
  return find
      .descendant(
        of: find.byKey(const Key('FlarkReadOnlyPreviewCodeBlock')),
        matching: find.byType(DecoratedBox),
      )
      .first;
}

void main() {
  group('FlarkMarkdownThemeData', () {
    test('light palette matches the pre-theming hard-coded colors', () {
      const light = FlarkMarkdownThemeData.light;
      expect(light, const FlarkMarkdownThemeData());
      expect(light.codeTextColor, const Color(0xFF17202A));
      expect(light.quoteTextColor, const Color(0xFF42526E));
      expect(light.linkColor, const Color(0xFF0057B8));
      expect(light.listMarkerColor, const Color(0xFF5B677A));
      expect(light.chromeLabelColor, const Color(0xFF42526E));
      expect(light.chromeSelectedLabelColor, const Color(0xFF17202A));
      expect(light.captionTextColor, const Color(0xFF5B6B7F));
      expect(light.errorTextColor, const Color(0xFFB3261E));
      expect(light.inlineCodeBackgroundColor, const Color(0xFFEFF3F7));
      expect(light.codeBlockBackgroundColor, const Color(0xFFF1F4F8));
      expect(light.quoteBackgroundColor, const Color(0xFFF8FAFC));
      expect(light.quoteRailColor, const Color(0xFF7A8CA3));
      expect(light.cardBackgroundColor, const Color(0xFFF8FAFC));
      expect(light.chipBackgroundColor, const Color(0xFFE2E8F0));
      expect(light.chipActiveBackgroundColor, const Color(0xFFD7DEE8));
      expect(light.menuBackgroundColor, const Color(0xFFFFFFFF));
      expect(light.menuShadowColor, const Color(0x1A000000));
      expect(light.borderColor, const Color(0xFFD7DEE8));
      expect(light.overlayControlBorderColor, const Color(0xFFB8C1CC));
      expect(light.tableHeaderBackgroundColor, const Color(0xFFF1F4F8));
      expect(light.tableRowBackgroundColor, const Color(0xFFFFFFFF));
      expect(light.tableDividerColor, const Color(0xFFE2E8F0));
      expect(light.checkboxCheckedColor, const Color(0xFF2E7D32));
      expect(light.checkboxBorderColor, const Color(0xFF7A8CA3));
      expect(light.checkboxFillColor, const Color(0xFFFFFFFF));
      expect(light.checkboxCheckmarkColor, const Color(0xFFFFFFFF));
      expect(light.cursorColor, const Color(0xFF006ADC));
      expect(light.syntaxTheme, FlarkCodeSyntaxThemeData.light);
      expect(light.syntaxTheme.commentColor, const Color(0xFF64748B));
      expect(light.syntaxTheme.stringColor, const Color(0xFF0F766E));
      expect(light.syntaxTheme.numberColor, const Color(0xFFB45309));
      expect(light.syntaxTheme.keywordColor, const Color(0xFF7C3AED));
      expect(light.syntaxTheme.functionColor, const Color(0xFF0369A1));
      expect(light.syntaxTheme.typeColor, const Color(0xFF047857));
      expect(light.syntaxTheme.attributeColor, const Color(0xFF1D4ED8));
      expect(light.syntaxTheme.variableColor, const Color(0xFFC2410C));
      expect(light.syntaxTheme.metaColor, const Color(0xFF475569));
      expect(light.syntaxTheme.deletionColor, const Color(0xFFB91C1C));
      expect(light.syntaxTheme.additionColor, const Color(0xFF047857));
    });

    test('fromBrightness resolves light and dark palettes', () {
      expect(
        FlarkMarkdownThemeData.fromBrightness(Brightness.light),
        FlarkMarkdownThemeData.light,
      );
      expect(
        FlarkMarkdownThemeData.fromBrightness(Brightness.dark),
        FlarkMarkdownThemeData.dark,
      );
      expect(FlarkMarkdownThemeData.dark, isNot(FlarkMarkdownThemeData.light));
    });

    test('typography overrides default to null and round-trip copyWith', () {
      const light = FlarkMarkdownThemeData.light;
      expect(light.codeTextStyle, isNull);
      expect(light.headingTextStyle, isNull);
      expect(light.linkTextStyle, isNull);
      expect(light.selectionColor, isNull);

      const mono = TextStyle(fontFamily: 'JetBrains Mono', fontSize: 13);
      final themed = light.copyWith(
        codeTextStyle: mono,
        heading2TextStyle: const TextStyle(color: Color(0xFF112233)),
        selectionColor: const Color(0x3300FF00),
      );
      expect(themed.codeTextStyle, mono);
      expect(themed.headingLevelTextStyle(2)!.color, const Color(0xFF112233));
      expect(themed.headingLevelTextStyle(3), isNull);
      expect(themed, isNot(light));
    });

    test('copyWith overrides a single field and preserves equality', () {
      const accent = Color(0xFF123456);
      final themed = FlarkMarkdownThemeData.light.copyWith(linkColor: accent);
      expect(themed.linkColor, accent);
      expect(themed.borderColor, FlarkMarkdownThemeData.light.borderColor);
      expect(themed, isNot(FlarkMarkdownThemeData.light));
      expect(
        themed.copyWith(linkColor: FlarkMarkdownThemeData.light.linkColor),
        FlarkMarkdownThemeData.light,
      );
    });
  });

  group('FlarkMarkdownTheme resolution', () {
    testWidgets('defaults to light, follows platform brightness, and '
        'prefers an ambient theme', (tester) async {
      final resolved = <FlarkMarkdownThemeData>[];
      Widget probe() {
        return Builder(
          builder: (context) {
            resolved.add(FlarkMarkdownTheme.of(context));
            return const SizedBox.shrink();
          },
        );
      }

      await tester.pumpWidget(probe());
      expect(resolved.removeLast(), FlarkMarkdownThemeData.light);

      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(platformBrightness: Brightness.dark),
          child: probe(),
        ),
      );
      expect(resolved.removeLast(), FlarkMarkdownThemeData.dark);

      final custom = FlarkMarkdownThemeData.light.copyWith(
        linkColor: const Color(0xFF123456),
      );
      await tester.pumpWidget(
        MediaQuery(
          data: const MediaQueryData(platformBrightness: Brightness.dark),
          child: FlarkMarkdownTheme(data: custom, child: probe()),
        ),
      );
      expect(resolved.removeLast(), custom);
    });
  });

  group('themed preview chrome', () {
    const fenced = 'Intro\n\n```dart\nfinal x = 1;\n```\n';

    testWidgets('code fences use the theme background', (tester) async {
      final controller = await _parsedController(fenced);
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdown(
            controller: controller,
            theme: FlarkMarkdownThemeData.dark,
          ),
        ),
      );

      expect(
        _decorationColor(tester, _previewCodeBlockDecoration()),
        FlarkMarkdownThemeData.dark.codeBlockBackgroundColor,
      );
    });

    testWidgets('without a theme the legacy light chrome is unchanged', (
      tester,
    ) async {
      final controller = await _parsedController(fenced);
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdown(controller: controller),
        ),
      );

      expect(
        _decorationColor(tester, _previewCodeBlockDecoration()),
        const Color(0xFFF1F4F8),
      );
    });

    testWidgets('an ambient FlarkMarkdownTheme themes the preview', (
      tester,
    ) async {
      final controller = await _parsedController('> quoted');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownTheme(
            data: FlarkMarkdownThemeData.dark,
            child: FlarkMarkdown(controller: controller),
          ),
        ),
      );

      final quoteBox = find
          .descendant(
            of: find.byKey(const Key('FlarkReadOnlyPreviewBlockquote')),
            matching: find.byType(DecoratedBox),
          )
          .first;
      expect(
        _decorationColor(tester, quoteBox),
        FlarkMarkdownThemeData.dark.quoteBackgroundColor,
      );
    });
  });

  group('typography theming', () {
    testWidgets('headings take merged color, family, and per-level size', (
      tester,
    ) async {
      final controller = await _parsedController('- x\n\n## Title');
      addTearDown(controller.dispose);

      final theme = FlarkMarkdownThemeData.light.copyWith(
        headingTextStyle: const TextStyle(
          color: Color(0xFF7C3AED),
          fontFamily: 'Fraunces',
        ),
        heading2TextStyle: const TextStyle(fontSize: 40),
      );
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            theme: theme,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      final heading = tester.widget<EditableText>(
        find.byWidgetPredicate(
          (widget) =>
              widget is EditableText && widget.controller.text == 'Title',
        ),
      );
      expect(heading.style.color, const Color(0xFF7C3AED));
      expect(heading.style.fontFamily, 'Fraunces');
      expect(heading.style.fontSize, 40);
      expect(heading.style.fontWeight, FontWeight.w700);
    });

    testWidgets('code fences take a custom code font', (tester) async {
      final controller = await _parsedController(
        'Intro\n\n```dart\nfinal x = 1;\n```\n',
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            theme: FlarkMarkdownThemeData.light.copyWith(
              codeTextStyle: const TextStyle(
                fontFamily: 'JetBrains Mono',
                fontSize: 12,
              ),
            ),
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      final body = tester.widget<EditableText>(
        find.descendant(
          of: find.byKey(const Key('FlarkLiveBlockCodeEditable')),
          matching: find.byType(EditableText),
        ),
      );
      expect(body.style.fontFamily, 'JetBrains Mono');
      expect(body.style.fontSize, 12);
      expect(body.style.color, FlarkMarkdownThemeData.light.codeTextColor);
    });

    testWidgets('links can drop the underline via linkTextStyle', (
      tester,
    ) async {
      final controller = await _parsedController('a [b](https://c.d) e');
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      TextStyle linkStyle() {
        final editable = tester.widget<EditableText>(
          find.byType(EditableText).first,
        );
        final span = editable.controller.buildTextSpan(
          context: tester.element(find.byType(EditableText).first),
          style: editable.style,
          withComposing: false,
        );
        TextStyle? found;
        span.visitChildren((child) {
          final style = child.style;
          if (child is TextSpan &&
              child.text == 'b' &&
              style != null &&
              style.color == FlarkMarkdownThemeData.light.linkColor) {
            found = style;
            return false;
          }
          return true;
        });
        expect(found, isNotNull, reason: 'link span present');
        return found!;
      }

      expect(linkStyle().decoration, TextDecoration.underline);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownTheme(
            data: FlarkMarkdownThemeData.light.copyWith(
              linkTextStyle: const TextStyle(decoration: TextDecoration.none),
            ),
            child: FlarkMarkdownEditor(
              controller: controller,
              style: const TextStyle(fontSize: 14),
            ),
          ),
        ),
      );
      await tester.pump();
      expect(linkStyle().decoration, TextDecoration.none);
    });

    testWidgets('bullet markers paint with listMarkerColor', (tester) async {
      final controller = await _parsedController('- bullet');
      addTearDown(controller.dispose);

      const marker = Color(0xFFAB47BC);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            theme: FlarkMarkdownThemeData.light.copyWith(
              listMarkerColor: marker,
            ),
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      final paint = tester.widget<CustomPaint>(
        find.descendant(
          of: find.byKey(const Key('FlarkLiveBlockListMarker')),
          matching: find.byType(CustomPaint),
        ),
      );
      expect((paint.painter as dynamic).color, marker);
    });

    testWidgets('selectionColor overrides the cursor-derived default', (
      tester,
    ) async {
      final controller = await _parsedController('- x\n\nplain');
      addTearDown(controller.dispose);

      const selection = Color(0x55FF8800);
      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            theme: FlarkMarkdownThemeData.light.copyWith(
              selectionColor: selection,
            ),
            style: const TextStyle(fontSize: 14),
          ),
        ),
      );
      await tester.pump();

      final editables = tester.widgetList<EditableText>(
        find.byType(EditableText),
      );
      expect(editables, isNotEmpty);
      for (final editable in editables) {
        expect(editable.selectionColor, selection);
      }
    });
  });

  group('themed live editor chrome', () {
    testWidgets('code fences in the live editor use the theme background', (
      tester,
    ) async {
      final controller = await _parsedController(
        'Intro\n\n```dart\nfinal x = 1;\n```\n',
      );
      addTearDown(controller.dispose);

      await tester.pumpWidget(
        Directionality(
          textDirection: TextDirection.ltr,
          child: FlarkMarkdownEditor(
            controller: controller,
            theme: FlarkMarkdownThemeData.dark,
          ),
        ),
      );
      await tester.pump();

      final fence = tester.widget<DecoratedBox>(
        find.byKey(const Key('FlarkLiveBlockCodeFence')),
      );
      expect(
        (fence.decoration as BoxDecoration).color,
        FlarkMarkdownThemeData.dark.codeBlockBackgroundColor,
      );
    });
  });
}
