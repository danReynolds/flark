import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/flark_advanced.dart';

import '../support/flark_test_paths.dart';

void main() {
  group('Flark markdown feature matrix', () {
    final libPath = flarkNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; feature matrix suite skipped', () {
        debugPrint('Skipped markdown feature matrix: native bridge missing.');
        expect(true, isTrue);
      });
      return;
    }

    final backend = FlarkNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    for (final featureCase in _featureCases) {
      test(
        '${featureCase.id} covers parser, projection, and render plan',
        () async {
          final result = await backend.parse(
            FlarkMarkdownParseRequest(
              revision: featureCase.id.hashCode,
              markdown: featureCase.markdown,
              profile: featureCase.profile,
            ),
          );

          _expectNoErrors(result, featureCase.id);
          _expectRangesValid(result, featureCase.markdown.length);

          final projection = FlarkProjection.fromParseResult(result);
          final displayText = projection.projectText(featureCase.markdown);
          final renderPlan = FlarkRenderPlan.fromParseResult(
            parseResult: result,
            projection: projection,
          );

          _expectContainsAll(
            _blockTypes(result),
            featureCase.blockTypes,
            '${featureCase.id} block types',
          );
          _expectContainsAll(
            _inlineTypes(result),
            featureCase.inlineTypes,
            '${featureCase.id} inline types',
          );
          _expectContainsAll(
            _hiddenTypes(result),
            featureCase.hiddenTypes,
            '${featureCase.id} hidden range types',
          );
          _expectContainsAll(
            _replacementTypes(result),
            featureCase.replacementTypes,
            '${featureCase.id} replacement range types',
          );
          _expectContainsAll(
            _overlayKinds(renderPlan),
            featureCase.overlayKinds,
            '${featureCase.id} overlay target kinds',
          );
          for (final fragment in featureCase.displayContains) {
            expect(displayText, contains(fragment), reason: featureCase.id);
          }
          for (final fragment in featureCase.displayOmits) {
            expect(
              displayText,
              isNot(contains(fragment)),
              reason: featureCase.id,
            );
          }
          featureCase.verify?.call(
            _MatrixArtifacts(
              result: result,
              projection: projection,
              displayText: displayText,
              renderPlan: renderPlan,
            ),
          );
        },
      );
    }
  });
}

final _featureCases = [
  _FeatureCase(
    id: 'atx_heading',
    markdown: '# Heading\n',
    blockTypes: const {'heading'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'Heading'},
    displayOmits: const {'#'},
    verify: (artifacts) {
      expect(
        artifacts.renderPlan.blocks.single.styleToken,
        FlarkRenderTextStyleToken.heading1,
      );
    },
  ),
  _FeatureCase(
    id: 'setext_heading',
    markdown: 'Heading\n=======\n',
    blockTypes: const {'heading'},
    displayContains: const {'Heading'},
    verify: (artifacts) {
      expect(
        artifacts.result.blocks
            .where((block) => block.type == 'heading')
            .single
            .attributes['level'],
        1,
      );
    },
  ),
  _FeatureCase(
    id: 'blockquote',
    markdown: '> quote\n',
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'quote'},
    displayOmits: const {'>'},
  ),
  _FeatureCase(
    id: 'multiline_blockquote',
    markdown: '> first\n> second\ncontinued\n',
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'first\nsecond\ncontinued'},
    displayOmits: const {'>'},
    verify: (artifacts) {
      expect(
        artifacts.result.blocks.where(
          (block) => block.kind == FlarkMarkdownBlockKind.blockquote,
        ),
        hasLength(1),
      );
      expect(
        artifacts.renderPlan.blocks.where(
          (block) => block.kind == FlarkMarkdownBlockKind.blockquote,
        ),
        hasLength(1),
      );
    },
  ),
  _FeatureCase(
    id: 'empty_blockquote',
    markdown: '> ',
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'>'},
  ),
  _FeatureCase(
    id: 'quoted_list_item',
    markdown: '> - item\n',
    blockTypes: const {'blockquote', 'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'item'},
    displayOmits: const {'>', '-'},
  ),
  _FeatureCase(
    id: 'marker_only_unordered_list_marker',
    markdown: '*',
    blockTypes: const {'paragraph'},
    displayContains: const {'*'},
    verify: (artifacts) {
      expect(artifacts.result.hiddenRanges, isEmpty);
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
    },
  ),
  _FeatureCase(
    id: 'marker_only_ordered_list_marker',
    markdown: '1.',
    blockTypes: const {'paragraph'},
    displayContains: const {'1.'},
    verify: (artifacts) {
      expect(artifacts.result.hiddenRanges, isEmpty);
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
    },
  ),
  _FeatureCase(
    id: 'unordered_list',
    markdown: '- item\n',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'item'},
    displayOmits: const {'-'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isNotEmpty);
      expect(
        artifacts.renderPlan.listItemBlocks.first.listItem!.kind,
        FlarkRenderListKind.unordered,
      );
    },
  ),
  _FeatureCase(
    id: 'ordered_list',
    markdown: '3. item\n',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'item'},
    displayOmits: const {'3.'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isNotEmpty);
      expect(
        artifacts.renderPlan.listItemBlocks.first.listItem!.kind,
        FlarkRenderListKind.ordered,
      );
    },
  ),
  _FeatureCase(
    id: 'task_list',
    markdown: '- [x] done\n',
    profile: FlarkMarkdownProfile.commonMarkGfm,
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    overlayKinds: const {'taskListItem'},
    displayContains: const {'done'},
    displayOmits: const {'[x]'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.taskListItemBlocks, isNotEmpty);
      expect(
        artifacts.renderPlan.taskListItemBlocks.first.taskListItem!.checked,
        isTrue,
      );
    },
  ),
  _FeatureCase(
    id: 'fenced_code',
    markdown: '```dart\nprint(1);\n```\n',
    blockTypes: const {'codeBlock'},
    overlayKinds: const {'codeBlock'},
    displayContains: const {'print(1);'},
    displayOmits: const {'```'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.codeBlocks, isNotEmpty);
      expect(artifacts.renderPlan.codeBlocks.first.codeBlock!.language, 'dart');
    },
  ),
  _FeatureCase(
    id: 'indented_code',
    markdown: '    code\n',
    blockTypes: const {'codeBlock'},
    displayContains: const {'code'},
  ),
  _FeatureCase(
    id: 'inline_styles',
    markdown: '**bold** *em* `code`\n',
    inlineTypes: const {'strong', 'emphasis', 'inlineCode'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'bold', 'em', 'code'},
    displayOmits: const {'**', '`'},
  ),
  _FeatureCase(
    id: 'escaped_delimiters',
    markdown: r'\*literal\* and \_plain\_',
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'*literal*', '_plain_'},
    displayOmits: const {r'\'},
  ),
  _FeatureCase(
    id: 'html_entities',
    markdown: 'A &amp; B &lt; C &#x1F600;\n',
    replacementTypes: const {'htmlEntity'},
    displayContains: const {'A & B < C 😀'},
    displayOmits: const {'&amp;', '&lt;', '&#x1F600;'},
  ),
  _FeatureCase(
    id: 'escaped_html_entity',
    markdown: r'\&amp; not entity',
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'&amp; not entity'},
    displayOmits: const {r'\'},
    verify: (artifacts) {
      expect(artifacts.result.replacementRanges, isEmpty);
    },
  ),
  _FeatureCase(
    id: 'html_entity_inside_hidden_link_destination',
    markdown: '[x](/a&amp;b)\n',
    inlineTypes: const {'link'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'link'},
    displayContains: const {'x'},
    displayOmits: const {'&amp;', '/a'},
    verify: (artifacts) {
      expect(artifacts.result.replacementRanges, isEmpty);
    },
  ),
  _FeatureCase(
    id: 'links_and_autolinks',
    markdown: '[site](https://example.com "T") and <https://a.test>\n',
    inlineTypes: const {'link'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'link'},
    displayContains: const {'site', 'https://a.test'},
    displayOmits: const {'](', '"T"'},
  ),
  _FeatureCase(
    id: 'link_with_escaped_destination_markers',
    markdown: '[foo](/bar\\* "ti\\*tle")\n',
    inlineTypes: const {'link'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'link'},
    displayContains: const {'foo'},
    displayOmits: const {'/bar', r'\*'},
  ),
  _FeatureCase(
    id: 'strikethrough',
    markdown: '~~gone~~\n',
    profile: FlarkMarkdownProfile.commonMarkGfm,
    inlineTypes: const {'strikethrough'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'gone'},
    displayOmits: const {'~~'},
  ),
  _FeatureCase(
    id: 'image',
    markdown: '![alt](https://example.com/a.png "T")\n',
    inlineTypes: const {'image'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'image'},
    displayContains: const {'alt'},
    displayOmits: const {'!['},
  ),
  _FeatureCase(
    id: 'linked_image',
    markdown: '[![moon](moon.jpg)](/uri)\n',
    inlineTypes: const {'link', 'image'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'link', 'image'},
    displayContains: const {'moon'},
    displayOmits: const {'![', 'moon.jpg', '/uri'},
  ),
  _FeatureCase(
    id: 'reference_link_definition',
    markdown: '[label]: https://example.com "T"\n\n[label]\n',
    inlineTypes: const {'link'},
    hiddenTypes: const {'referenceDefinition'},
    overlayKinds: const {'link'},
    displayContains: const {'label'},
    displayOmits: const {'https://example.com'},
  ),
  _FeatureCase(
    id: 'thematic_break',
    markdown: '---\n',
    blockTypes: const {'thematicBreak'},
  ),
  _FeatureCase(
    id: 'gfm_table',
    markdown: '| A | B |\n| :- | -: |\n| x | y |\n',
    profile: FlarkMarkdownProfile.commonMarkGfm,
    blockTypes: const {'table'},
    overlayKinds: const {'table'},
    displayContains: const {'A', 'B', 'x', 'y'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.tableBlocks, isNotEmpty);
      expect(artifacts.renderPlan.tableBlocks.first.table!.columnAlignments, [
        FlarkRenderTableColumnAlignment.left,
        FlarkRenderTableColumnAlignment.right,
      ]);
    },
  ),
  _FeatureCase(
    id: 'raw_html',
    markdown: '<div>raw</div>\n\ninline <span>x</span>\n',
    blockTypes: const {'htmlBlock'},
    inlineTypes: const {'htmlInline'},
    hiddenTypes: const {'rawHtml'},
    displayContains: const {'inline', 'x'},
    displayOmits: const {'<div>', '<span>'},
  ),
];

final class _FeatureCase {
  const _FeatureCase({
    required this.id,
    required this.markdown,
    this.profile = FlarkMarkdownProfile.commonMarkCore,
    this.blockTypes = const {},
    this.inlineTypes = const {},
    this.hiddenTypes = const {},
    this.replacementTypes = const {},
    this.overlayKinds = const {},
    this.displayContains = const {},
    this.displayOmits = const {},
    this.verify,
  });

  final String id;
  final String markdown;
  final FlarkMarkdownProfile profile;
  final Set<String> blockTypes;
  final Set<String> inlineTypes;
  final Set<String> hiddenTypes;
  final Set<String> replacementTypes;
  final Set<String> overlayKinds;
  final Set<String> displayContains;
  final Set<String> displayOmits;
  final void Function(_MatrixArtifacts artifacts)? verify;
}

final class _MatrixArtifacts {
  const _MatrixArtifacts({
    required this.result,
    required this.projection,
    required this.displayText,
    required this.renderPlan,
  });

  final FlarkMarkdownParseResult result;
  final FlarkProjection projection;
  final String displayText;
  final FlarkRenderPlan renderPlan;
}

void _expectNoErrors(FlarkMarkdownParseResult result, String id) {
  final errors = result.diagnostics.where(
    (diagnostic) => diagnostic.extensions['isError'] == true,
  );
  expect(
    errors,
    isEmpty,
    reason: errors
        .map((diagnostic) => '$id ${diagnostic.code}: ${diagnostic.message}')
        .join('\n'),
  );
}

void _expectRangesValid(FlarkMarkdownParseResult result, int textLength) {
  bool validRange(FlarkSourceRange range) {
    return range.start >= 0 &&
        range.start < range.end &&
        range.end <= textLength;
  }

  for (final block in _allBlocks(result.blocks)) {
    expect(
      validRange(block.sourceRange),
      isTrue,
      reason: 'invalid block range ${block.type} ${block.sourceRange}',
    );
  }
  for (final token in result.inlineTokens) {
    expect(
      validRange(token.sourceRange),
      isTrue,
      reason: 'invalid inline range ${token.type} ${token.sourceRange}',
    );
  }
  for (final range in result.hiddenRanges) {
    expect(
      validRange(range.sourceRange),
      isTrue,
      reason: 'invalid hidden range ${range.type} ${range.sourceRange}',
    );
  }
  for (final range in result.replacementRanges) {
    expect(
      validRange(range.sourceRange),
      isTrue,
      reason: 'invalid replacement range ${range.type} ${range.sourceRange}',
    );
    expect(
      range.replacementText,
      isNotEmpty,
      reason: 'empty replacement text for ${range.sourceRange}',
    );
  }
  for (final zone in result.ambiguityZones) {
    expect(
      validRange(zone.sourceRange),
      isTrue,
      reason: 'invalid ambiguity zone ${zone.type} ${zone.sourceRange}',
    );
  }
}

Iterable<FlarkMarkdownBlockNode> _allBlocks(
  Iterable<FlarkMarkdownBlockNode> blocks,
) sync* {
  for (final block in blocks) {
    yield block;
    yield* _allBlocks(block.children);
  }
}

Set<String> _blockTypes(FlarkMarkdownParseResult result) {
  return _allBlocks(result.blocks).map((block) => block.type).toSet();
}

Set<String> _inlineTypes(FlarkMarkdownParseResult result) {
  return result.inlineTokens.map((token) => token.type).toSet();
}

Set<String> _hiddenTypes(FlarkMarkdownParseResult result) {
  return result.hiddenRanges.map((range) => range.type).toSet();
}

Set<String> _replacementTypes(FlarkMarkdownParseResult result) {
  return result.replacementRanges.map((range) => range.type).toSet();
}

Set<String> _overlayKinds(FlarkRenderPlan renderPlan) {
  return renderPlan
      .overlayPlan()
      .targets
      .map((target) => target.kind.name)
      .toSet();
}

void _expectContainsAll(
  Set<String> actual,
  Set<String> expected,
  String reason,
) {
  for (final item in expected) {
    expect(actual, contains(item), reason: '$reason missing $item in $actual');
  }
}
