import 'package:example/v3_engine_lab.dart';
import 'package:flutter/material.dart' show Key, SelectableText;
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('reference stress fixtures share one exact editable tail', () {
    for (final referenceCount in const [4096, 100000]) {
      final source = buildV3EngineLabLeadingReferenceSeed(referenceCount);
      final prefix = source.substring(
        0,
        source.length - v3EngineLabEditableTailSource.length,
      );

      expect(source, endsWith(v3EngineLabEditableTailSource));
      expect('\n'.allMatches(prefix), hasLength(referenceCount));
      expect(prefix, startsWith('[flark]: /target\n'));
    }
  });

  test('reference stress fixtures select the projected tail editor', () {
    expect(V3EngineLabSeed.values, const [
      V3EngineLabSeed.small,
      V3EngineLabSeed.multiBlockParagraph,
      V3EngineLabSeed.atxHeading,
      V3EngineLabSeed.setextHeading,
      V3EngineLabSeed.thematicBreak,
      V3EngineLabSeed.fencedCode,
      V3EngineLabSeed.indentedCode,
      V3EngineLabSeed.blockQuote,
      V3EngineLabSeed.bulletList,
      V3EngineLabSeed.orderedList,
      V3EngineLabSeed.references4096,
      V3EngineLabSeed.references100000,
      V3EngineLabSeed.oneMebibyte,
      V3EngineLabSeed.tenMebibytes,
    ]);
    expect(V3EngineLabSeed.small.leadingReferenceCount, 1);
    expect(V3EngineLabSeed.references4096.leadingReferenceCount, 4096);
    expect(V3EngineLabSeed.references100000.leadingReferenceCount, 100000);
    expect(V3EngineLabSeed.small.usesProjectedTailEditor, isTrue);
    expect(V3EngineLabSeed.references4096.usesProjectedTailEditor, isTrue);
    expect(V3EngineLabSeed.references100000.usesProjectedTailEditor, isTrue);
    expect(V3EngineLabSeed.setextHeading.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.setextHeading.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.thematicBreak.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.thematicBreak.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.thematicBreak.label, contains('atomic marker-free'));
    expect(V3EngineLabSeed.indentedCode.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.indentedCode.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.indentedCode.label, contains('marker-free'));
    expect(V3EngineLabSeed.blockQuote.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.blockQuote.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.blockQuote.label, contains('depth-one'));
    expect(V3EngineLabSeed.bulletList.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.bulletList.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.bulletList.label, contains('marker-free'));
    expect(V3EngineLabSeed.orderedList.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.orderedList.usesManagedEditor, isTrue);
    expect(V3EngineLabSeed.orderedList.label, contains('exact marker'));
    expect(V3EngineLabSeed.oneMebibyte.usesProjectedTailEditor, isFalse);
    expect(V3EngineLabSeed.tenMebibytes.usesProjectedTailEditor, isFalse);
  });

  test('the certified tail display omits canonical Markdown delimiters', () {
    expect(v3EngineLabEditableTailSource, contains('**Bold**'));
    expect(v3EngineLabEditableTailSource, contains('_emphasis_'));
    expect(v3EngineLabEditableTailSource, contains('`code`'));
    expect(v3EngineLabEditableTailSource, contains('~~strike~~'));
    expect(v3EngineLabEditableTailSource, contains('<https://commonmark.org>'));
    expect(v3EngineLabEditableTailSource, contains('<hello@example.com>'));
    expect(v3EngineLabEditableTailSource, contains('&copy;'));
    expect(v3EngineLabEditableTailSource, contains('&ngE;'));
    expect(
      v3EngineLabEditableTailSource,
      contains('<https://e.test/?q=&amp;>'),
    );
    expect(
      v3EngineLabEditableTailDisplay,
      contains('Bold, emphasis, code, strike'),
    );
    expect(v3EngineLabEditableTailDisplay, isNot(contains('**')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('_')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('`')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('~')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('<')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('>')));
    expect(v3EngineLabEditableTailDisplay, contains('https://commonmark.org'));
    expect(v3EngineLabEditableTailDisplay, contains('hello@example.com'));
    expect(v3EngineLabEditableTailDisplay, contains('©'));
    expect(v3EngineLabEditableTailDisplay, contains('≧\u{338}'));
    expect(v3EngineLabEditableTailDisplay, contains('https://e.test/?q=&'));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('&copy;')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('&ngE;')));
    expect(v3EngineLabEditableTailDisplay, isNot(contains('&amp;')));
  });

  test('Setext fixture separates marker-free display from canonical CRLF', () {
    expect(v3EngineLabSetextHeadingSource, '**β😀** live _heading_\r\n---\r\n');
    expect(v3EngineLabSetextHeadingSource, endsWith('\r\n---\r\n'));
    expect(v3EngineLabSetextHeadingDisplay, 'β😀 live heading');
    expect(v3EngineLabSetextHeadingDisplay, isNot(contains('**')));
    expect(v3EngineLabSetextHeadingDisplay, isNot(contains('_')));
    expect(v3EngineLabSetextHeadingDisplay, isNot(contains('---')));
    expect(v3EngineLabSetextHeadingDisplay, isNot(contains('\r\n')));
  });

  test('thematic-break fixture is one marker-free atomic projection', () {
    expect(v3EngineLabThematicBreakSource, '  * * * \r\n');
    expect(v3EngineLabThematicBreakSource, contains('* * *'));
    expect(v3EngineLabThematicBreakDisplay, isEmpty);
    expect(v3EngineLabThematicBreakDisplay, isNot(contains('*')));
  });

  test('indented-code fixture separates display from canonical prefixes', () {
    expect(v3EngineLabIndentedCodeSource, startsWith('    final message'));
    expect(v3EngineLabIndentedCodeSource, contains('\n      print'));
    expect(v3EngineLabIndentedCodeDisplay, startsWith('final message'));
    expect(v3EngineLabIndentedCodeDisplay, contains('\n  print'));
    expect(v3EngineLabIndentedCodeDisplay, contains('**literal Markdown**'));
    expect(v3EngineLabIndentedCodeDisplay, isNot(startsWith('    ')));
  });

  test('block-quote fixture is marker-free without overstating scope', () {
    expect(v3EngineLabBlockQuoteSource, startsWith('> '));
    expect(v3EngineLabBlockQuoteSource, contains('\n> Canonical'));
    expect(v3EngineLabBlockQuoteDisplay, isNot(contains('> ')));
    expect(
      v3EngineLabBlockQuoteDisplay,
      'Parser-certified quote text stays marker-free.\n'
      'Canonical quote prefixes remain in exact source.\n',
    );
    expect(v3EngineLabBlockQuoteScope, contains('depth-one'));
    expect(v3EngineLabBlockQuoteScope, contains('single-paragraph'));
    expect(
      v3EngineLabBlockQuoteScope,
      contains('Inline styles inside quotes are not yet composed'),
    );
  });

  test(
    'bullet-list fixture separates selected display from canonical source',
    () {
      expect(v3EngineLabBulletListSource, startsWith('  - α😀'));
      expect(
        v3EngineLabBulletListSource,
        contains('\r\n  - Edit **this** _live_ `list` item.'),
      );
      expect(v3EngineLabBulletListSource, endsWith('-   '));
      expect(v3EngineLabBulletListFirstDisplay, 'α😀 first item\n');
      expect(v3EngineLabBulletListSecondDisplay, 'Edit this live list item.\n');
      expect(v3EngineLabBulletListTerminalDisplay, isEmpty);
      expect(v3EngineLabBulletListFirstDisplay, isNot(contains('- ')));
      expect(v3EngineLabBulletListSecondDisplay, isNot(contains('- ')));
      expect(v3EngineLabBulletListSecondDisplay, isNot(contains('**')));
      expect(v3EngineLabBulletListSecondDisplay, isNot(contains('_')));
      expect(v3EngineLabBulletListSecondDisplay, isNot(contains('`')));
      expect(
        v3EngineLabBulletListScope,
        contains('depth-one tight bullet list'),
      );
      expect(v3EngineLabBulletListScope, contains('bold, emphasis'));
      expect(v3EngineLabBulletListScope, contains('nested, loose, ordered'));
    },
  );

  test('ordered-list fixture keeps exact marker out of editable content', () {
    expect(v3EngineLabOrderedListSource, '007) alpha\r\n9) beta\r\n');
    expect(v3EngineLabOrderedListSource, startsWith('007)'));
    expect(v3EngineLabOrderedListDisplay, 'alpha\n');
    expect(v3EngineLabOrderedListDisplay, isNot(contains('007)')));
    expect(
      v3EngineLabOrderedListScope,
      contains('top-level, depth-one tight ordered list'),
    );
    expect(v3EngineLabOrderedListScope, contains('nested, loose, and task'));
  });

  test('bounded edit delta does not split a surrogate pair', () {
    final delta = computeV3EngineLabEditDelta('a😀z', 'a😁z');

    expect(delta, isNotNull);
    expect(delta!.startUtf16, 1);
    expect(delta.endUtf16, 3);
    expect(delta.replacement, '😁');
  });

  test('bounded edit delta trims a shared prefix and suffix', () {
    final delta = computeV3EngineLabEditDelta(
      'before old after',
      'before new after',
    );

    expect(delta, isNotNull);
    expect(delta!.startUtf16, 7);
    expect(delta.endUtf16, 10);
    expect(delta.replacement, 'new');
  });

  testWidgets('lab exposes the bounded M1.1 feedback checkpoints', (
    tester,
  ) async {
    await tester.pumpWidget(const V3EngineLabApp(openOnStart: false));

    expect(
      find.text('Flark v3 · Feedback Checkpoints A + B + C'),
      findsOneWidget,
    );
    expect(find.textContaining('M1.1 GRAMMAR LIMIT'), findsOneWidget);
    expect(
      find.textContaining('one parser-certified atomic divider'),
      findsOneWidget,
    );
    expect(
      find.text('Feedback Checkpoint B · persistent incremental SourceFacts'),
      findsOneWidget,
    );
    expect(find.text('Run Checkpoint B proof'), findsOneWidget);
    expect(
      find.text('Feedback Checkpoint C · exact-base live loop'),
      findsOneWidget,
    );
    expect(find.textContaining('production Worker/isolate'), findsOneWidget);
    expect(find.textContaining('STABLE/WAITING'), findsOneWidget);
    for (final seed in V3EngineLabSeed.values) {
      expect(find.text('Open ${seed.label}'), findsOneWidget);
    }
    expect(find.text('Open 4,096 leading refs + live tail'), findsOneWidget);
    expect(find.text('Open 100,000 leading refs + live tail'), findsOneWidget);
    expect(find.text('Cold full open → exact'), findsWidgets);
    expect(find.text('Selected fixture'), findsOneWidget);
    expect(find.text('Visible block range'), findsOneWidget);
    expect(find.text('Visible range work'), findsOneWidget);
    expect(find.textContaining('not an internal parser queue'), findsOneWidget);
    expect(find.text('Build mode'), findsOneWidget);
    expect(find.text('debug'), findsOneWidget);
    expect(find.text('Source revision'), findsOneWidget);
    expect(find.text('Certified revision'), findsOneWidget);
    expect(find.text('Structure revision'), findsOneWidget);
    expect(find.text('Source current'), findsOneWidget);
    expect(find.text('Structure current'), findsOneWidget);
    expect(find.text('Inline presentation generation'), findsOneWidget);
    expect(find.text('Recovery available'), findsOneWidget);
    expect(find.text('Worker generation'), findsNothing);
    expect(find.text('Installed host'), findsNothing);
    expect(find.byKey(const Key('v3-engine-lab-editor')), findsOneWidget);
    expect(
      find.byKey(const Key('v3-engine-lab-inline-presentation-generation')),
      findsOneWidget,
    );
    final exactSource = tester.widget<SelectableText>(
      find.byKey(const Key('v3-engine-lab-exact-source')),
    );
    expect(exactSource.data, v3EngineLabEditableTailSource);
    expect(exactSource.data, contains('**Bold**'));
    expect(exactSource.data, contains('_emphasis_'));
    expect(exactSource.data, contains('`code`'));
    expect(exactSource.data, contains('&copy;'));
    expect(exactSource.data, contains('&ngE;'));
    expect(exactSource.data, contains('<https://e.test/?q=&amp;>'));
  });
}
