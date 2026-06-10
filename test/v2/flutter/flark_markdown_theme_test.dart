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
