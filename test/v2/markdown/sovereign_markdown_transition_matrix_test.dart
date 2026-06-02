import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor_v2.dart';

import '../support/sovereign_test_paths.dart';

void main() {
  group('Sovereign markdown transition matrix', () {
    final libPath = sovereignNativeBridgeLibraryPathForPlatform();

    if (libPath.isEmpty || !File(libPath).existsSync()) {
      test('native bridge not built; transition matrix suite skipped', () {
        debugPrint(
          'Skipped markdown transition matrix: native bridge missing.',
        );
        expect(true, isTrue);
      });
      return;
    }

    final backend = SovereignNativeComrakParseBackend.withNativeBridge(
      overrideLibraryPath: libPath,
    );

    for (final transitionCase in _transitionCases) {
      test(
        '${transitionCase.id} preserves the expected boundary state',
        () async {
          final result = await backend.parse(
            SovereignMarkdownParseRequest(
              revision: transitionCase.id.hashCode,
              markdown: transitionCase.markdown,
              profile: transitionCase.profile,
            ),
          );

          final projection = SovereignProjection.fromParseResult(result);
          final displayText = projection.projectText(transitionCase.markdown);
          final renderPlan = SovereignRenderPlan.fromParseResult(
            parseResult: result,
            projection: projection,
          );

          _expectNoErrors(result, transitionCase.id);
          _expectRangesValid(result, transitionCase.markdown.length);
          _expectContainsAll(
            _blockTypes(result),
            transitionCase.blockTypes,
            '${transitionCase.id} block types',
          );
          _expectContainsAll(
            _inlineTypes(result),
            transitionCase.inlineTypes,
            '${transitionCase.id} inline types',
          );
          _expectContainsAll(
            _hiddenTypes(result),
            transitionCase.hiddenTypes,
            '${transitionCase.id} hidden types',
          );
          _expectContainsAll(
            _replacementTypes(result),
            transitionCase.replacementTypes,
            '${transitionCase.id} replacement types',
          );
          _expectContainsAll(
            _overlayKinds(renderPlan),
            transitionCase.overlayKinds,
            '${transitionCase.id} overlay kinds',
          );
          for (final fragment in transitionCase.displayContains) {
            expect(displayText, contains(fragment), reason: transitionCase.id);
          }
          for (final fragment in transitionCase.displayOmits) {
            expect(
              displayText,
              isNot(contains(fragment)),
              reason: transitionCase.id,
            );
          }
          transitionCase.verify?.call(
            _TransitionArtifacts(
              result: result,
              displayText: displayText,
              renderPlan: renderPlan,
            ),
          );
        },
      );
    }
  });
}

final _transitionCases = [
  _TransitionCase(
    id: 'atx_heading_empty_marker',
    markdown: '#',
    blockTypes: const {'paragraph'},
    displayContains: const {'#'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'atx_heading_marker_space',
    markdown: '# ',
    blockTypes: const {'heading'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'#'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(
        artifacts.renderPlan.blocks.single.styleToken,
        SovereignRenderTextStyleToken.heading1,
      );
    },
  ),
  _TransitionCase(
    id: 'escaped_heading_marker_stays_paragraph',
    markdown: r'\# Heading',
    blockTypes: const {'paragraph'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'# Heading'},
    verify: (artifacts) {
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.heading),
        isEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'blockquote_marker_only',
    markdown: '>',
    blockTypes: const {'paragraph'},
    displayContains: const {'>'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'blockquote_marker_space',
    markdown: '> ',
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'>'},
    verify: (artifacts) => expect(artifacts.displayText, isEmpty),
  ),
  _TransitionCase(
    id: 'blockquote_lazy_continuation',
    markdown: '> first\ncontinued',
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'first\ncontinued'},
    displayOmits: const {'>'},
    verify: (artifacts) {
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.blockquote),
        hasLength(1),
      );
    },
  ),
  _TransitionCase(
    id: 'blockquote_blank_line_ends_lazy_continuation',
    markdown: '> first\n\ncontinued',
    blockTypes: const {'blockquote', 'paragraph'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'first', 'continued'},
    verify: (artifacts) {
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.blockquote),
        hasLength(1),
      );
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.paragraph),
        isNotEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'github_alert_marker_stays_literal_blockquote',
    markdown: '> [!NOTE]\n> useful',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'blockquote'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'[!NOTE]\nuseful'},
    displayOmits: const {'>'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.codeBlocks, isEmpty);
      expect(artifacts.renderPlan.taskListItemBlocks, isEmpty);
      expect(_overlayKinds(artifacts.renderPlan), isEmpty);
    },
  ),
  _TransitionCase(
    id: 'unordered_marker_only',
    markdown: '*',
    blockTypes: const {'paragraph'},
    displayContains: const {'*'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'unordered_marker_space',
    markdown: '* ',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'*'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
      expect(
        artifacts.renderPlan.listItemBlocks.single.listItem!.kind,
        SovereignRenderListKind.unordered,
      );
    },
  ),
  _TransitionCase(
    id: 'escaped_unordered_marker_stays_paragraph',
    markdown: r'\* not a list',
    blockTypes: const {'paragraph'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'* not a list'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'three_space_indented_unordered_marker_space',
    markdown: '   - item',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'item'},
    displayOmits: const {'-'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'four_space_indented_unordered_marker_is_code',
    markdown: '    - item',
    blockTypes: const {'codeBlock'},
    displayContains: const {'item'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'tab_indented_unordered_marker_is_code',
    markdown: '\t- item',
    blockTypes: const {'codeBlock'},
    displayContains: const {'item'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'space_tab_indented_unordered_marker_is_code',
    markdown: ' \t- item',
    blockTypes: const {'codeBlock'},
    displayContains: const {'item'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'four_space_indented_blockquote_marker_is_code',
    markdown: '    > quote',
    blockTypes: const {'codeBlock'},
    displayContains: const {'quote'},
    verify: (artifacts) {
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.blockquote),
        isEmpty,
      );
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'unordered_list_lazy_continuation',
    markdown: '- first\ncontinued',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'first\ncontinued'},
    displayOmits: const {'-'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'ordered_marker_only',
    markdown: '1.',
    blockTypes: const {'paragraph'},
    displayContains: const {'1.'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'ordered_marker_space',
    markdown: '1. ',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'1.'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
      expect(
        artifacts.renderPlan.listItemBlocks.single.listItem!.kind,
        SovereignRenderListKind.ordered,
      );
    },
  ),
  _TransitionCase(
    id: 'escaped_ordered_marker_stays_paragraph',
    markdown: r'1\. not a list',
    blockTypes: const {'paragraph'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'1. not a list'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'ordered_marker_parenthesis_space',
    markdown: '1) ',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'1)'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
      expect(
        artifacts.renderPlan.listItemBlocks.single.listItem!.kind,
        SovereignRenderListKind.ordered,
      );
    },
  ),
  _TransitionCase(
    id: 'ordered_marker_nine_digits_space',
    markdown: '123456789. ',
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayOmits: const {'123456789.'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'ordered_marker_ten_digits_stays_paragraph',
    markdown: '1234567890. ',
    blockTypes: const {'paragraph'},
    displayContains: const {'1234567890.'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.listItemBlocks, isEmpty);
      expect(artifacts.result.hiddenRanges, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'task_marker_partial_open',
    markdown: '- [',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'['},
    overlayKinds: const {},
    verify: (artifacts) {
      expect(artifacts.renderPlan.taskListItemBlocks, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'task_marker_complete_empty',
    markdown: '- [ ] ',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'listItem'},
    hiddenTypes: const {'markdownMarker'},
    overlayKinds: const {'taskListItem'},
    displayOmits: const {'[ ]'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.taskListItemBlocks, hasLength(1));
      expect(
        artifacts.renderPlan.taskListItemBlocks.single.taskListItem!.checked,
        isFalse,
      );
    },
  ),
  _TransitionCase(
    id: 'fenced_code_partial_two_backticks',
    markdown: '``',
    blockTypes: const {'paragraph'},
    displayContains: const {'``'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'fenced_code_open_fence',
    markdown: '```',
    blockTypes: const {'codeBlock'},
    overlayKinds: const {'codeBlock'},
    displayOmits: const {'```'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'tilde_fenced_code_open_fence',
    markdown: '~~~',
    blockTypes: const {'codeBlock'},
    overlayKinds: const {'codeBlock'},
    displayOmits: const {'~~~'},
    verify: (artifacts) {
      expect(artifacts.displayText, isEmpty);
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
    },
  ),
  _TransitionCase(
    id: 'fenced_code_backtick_info_with_backtick_stays_paragraph',
    markdown: '```bad`info',
    blockTypes: const {'paragraph'},
    displayContains: const {'```bad`info'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.codeBlocks, isEmpty);
      expect(artifacts.result.hiddenRanges, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'four_space_indented_closing_fence_stays_code_content',
    markdown: '```\ncode\n    ```\nafter',
    blockTypes: const {'codeBlock'},
    overlayKinds: const {'codeBlock'},
    displayContains: const {'code', '```', 'after'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.paragraph),
        isEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'three_space_indented_closing_fence_closes',
    markdown: '```\ncode\n   ```\nafter',
    blockTypes: const {'codeBlock', 'paragraph'},
    overlayKinds: const {'codeBlock'},
    displayContains: const {'code', 'after'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.codeBlocks, hasLength(1));
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.paragraph),
        isNotEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'thematic_break_partial_two_dashes',
    markdown: '--',
    blockTypes: const {'paragraph'},
    displayContains: const {'--'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'thematic_break_complete',
    markdown: '---',
    blockTypes: const {'thematicBreak'},
  ),
  _TransitionCase(
    id: 'dash_line_after_paragraph_is_setext_heading',
    markdown: 'Title\n---',
    blockTypes: const {'heading'},
    displayContains: const {'Title'},
    verify: (artifacts) {
      expect(
        _allBlocks(artifacts.result.blocks).any(
          (block) => block.kind == SovereignMarkdownBlockKind.thematicBreak,
        ),
        isFalse,
      );
      expect(
        artifacts.renderPlan.blocks.single.styleToken,
        SovereignRenderTextStyleToken.heading2,
      );
    },
  ),
  _TransitionCase(
    id: 'dash_line_after_blank_is_thematic_break',
    markdown: 'Title\n\n---',
    blockTypes: const {'paragraph', 'thematicBreak'},
    displayContains: const {'Title'},
    verify: (artifacts) {
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).any((block) => block.kind == SovereignMarkdownBlockKind.heading),
        isFalse,
      );
    },
  ),
  _TransitionCase(
    id: 'table_partial_header_only',
    markdown: '| A | B |',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'paragraph'},
    displayContains: const {'A', 'B'},
    verify: (artifacts) => expect(artifacts.renderPlan.tableBlocks, isEmpty),
  ),
  _TransitionCase(
    id: 'table_complete_separator',
    markdown: '| A | B |\n| --- | --- |\n',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'table'},
    overlayKinds: const {'table'},
    displayContains: const {'A', 'B'},
    verify: (artifacts) =>
        expect(artifacts.renderPlan.tableBlocks, hasLength(1)),
  ),
  _TransitionCase(
    id: 'table_header_delimiter_cell_mismatch_stays_paragraph',
    markdown: '| A | B |\n| --- |\n| x | y |\n',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'paragraph'},
    displayContains: const {'| A | B |', '| --- |'},
    verify: (artifacts) {
      expect(artifacts.renderPlan.tableBlocks, isEmpty);
      expect(
        _allBlocks(
          artifacts.result.blocks,
        ).where((block) => block.kind == SovereignMarkdownBlockKind.table),
        isEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'table_body_cell_count_variance_keeps_header_columns',
    markdown: '| A | B |\n| --- | --- |\n| x |\n| y | z | ignored |\n',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'table'},
    overlayKinds: const {'table'},
    displayContains: const {'A', 'B', 'x', 'y', 'z'},
    verify: (artifacts) {
      final table = artifacts.renderPlan.tableBlocks.single.table!;
      expect(table.columnAlignments, hasLength(2));
    },
  ),
  _TransitionCase(
    id: 'strong_partial_open',
    markdown: '**',
    blockTypes: const {'paragraph'},
    displayContains: const {'**'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'strong_complete',
    markdown: '**bold**',
    inlineTypes: const {'strong'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'bold'},
    displayOmits: const {'**'},
  ),
  _TransitionCase(
    id: 'strong_missing_closing_marker_stays_literal',
    markdown: '**wow*',
    blockTypes: const {'paragraph'},
    displayContains: const {'**wow*'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'strong_missing_opening_marker_stays_literal',
    markdown: '*wow**',
    blockTypes: const {'paragraph'},
    displayContains: const {'*wow**'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'underscore_strong_missing_closing_marker_stays_literal',
    markdown: '__wow_',
    blockTypes: const {'paragraph'},
    displayContains: const {'__wow_'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'underscore_strong_missing_opening_marker_stays_literal',
    markdown: '_wow__',
    blockTypes: const {'paragraph'},
    displayContains: const {'_wow__'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'triple_delimiter_single_closer_stays_literal',
    markdown: '***wow*',
    blockTypes: const {'paragraph'},
    displayContains: const {'***wow*'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'triple_underscore_single_closer_stays_literal',
    markdown: '___wow_',
    blockTypes: const {'paragraph'},
    displayContains: const {'___wow_'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'strong_extra_trailing_marker_keeps_marker_visible',
    markdown: '**wow***',
    blockTypes: const {'paragraph'},
    inlineTypes: const {'strong'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'wow*'},
    displayOmits: const {'**'},
    verify: (artifacts) => expect(artifacts.displayText, 'wow*'),
  ),
  _TransitionCase(
    id: 'inline_code_partial_open',
    markdown: '`',
    blockTypes: const {'paragraph'},
    displayContains: const {'`'},
    verify: _expectNoHiddenRanges,
  ),
  _TransitionCase(
    id: 'inline_code_double_backtick_partial_close_stays_literal',
    markdown: '``code`',
    blockTypes: const {'paragraph'},
    displayContains: const {'``code`'},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'inline_code_complete',
    markdown: '`code`',
    inlineTypes: const {'inlineCode'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'code'},
    displayOmits: const {'`'},
  ),
  _TransitionCase(
    id: 'inline_code_double_backtick_can_contain_single_backtick',
    markdown: '`` ` ``',
    inlineTypes: const {'inlineCode'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'`'},
    displayOmits: const {'``'},
  ),
  _TransitionCase(
    id: 'intraword_underscore_stays_literal',
    markdown: 'foo_bar_baz',
    blockTypes: const {'paragraph'},
    displayContains: const {'foo_bar_baz'},
    verify: (artifacts) {
      expect(artifacts.result.inlineTokens, isEmpty);
      expect(artifacts.result.hiddenRanges, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'intraword_asterisk_can_emphasize',
    markdown: 'foo*bar*baz',
    blockTypes: const {'paragraph'},
    inlineTypes: const {'emphasis'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'foobarbaz'},
    displayOmits: const {'*'},
  ),
  _TransitionCase(
    id: 'link_partial_destination_open',
    markdown: '[label](',
    displayContains: const {'[label]('},
    overlayKinds: const {},
  ),
  _TransitionCase(
    id: 'link_partial_destination_text_stays_literal',
    markdown: '[label](url',
    blockTypes: const {'paragraph'},
    displayContains: const {'[label](url'},
    overlayKinds: const {},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'link_complete',
    markdown: '[label](https://example.com)',
    inlineTypes: const {'link'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'link'},
    displayContains: const {'label'},
    displayOmits: const {'https://example.com', ']('},
  ),
  _TransitionCase(
    id: 'collapsed_reference_link_resolves_after_definition',
    markdown: '[label][]\n\n[label]: https://example.com',
    inlineTypes: const {'link'},
    hiddenTypes: const {'inlineMarker', 'referenceDefinition'},
    overlayKinds: const {'link'},
    displayContains: const {'label'},
    displayOmits: const {'https://example.com'},
  ),
  _TransitionCase(
    id: 'unresolved_reference_link_stays_literal',
    markdown: '[label][]',
    displayContains: const {'[label][]'},
    overlayKinds: const {},
    verify: (artifacts) {
      expect(artifacts.result.inlineTokens, isEmpty);
      expect(artifacts.result.hiddenRanges, isEmpty);
    },
  ),
  _TransitionCase(
    id: 'escaped_reference_definition_stays_paragraph',
    markdown: r'\[foo]: /url',
    blockTypes: const {'paragraph'},
    hiddenTypes: const {'markdownMarker'},
    displayContains: const {'[foo]: /url'},
    verify: (artifacts) {
      expect(
        artifacts.result.hiddenRanges.where(
          (range) =>
              range.kind ==
              SovereignMarkdownHiddenRangeKind.referenceDefinition,
        ),
        isEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'github_footnote_syntax_stays_source_visible',
    markdown: 'Text[^1]\n\n[^1]: Footnote',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    blockTypes: const {'paragraph'},
    displayContains: const {'Text[^1]', '[^1]: Footnote'},
    overlayKinds: const {},
    verify: (artifacts) {
      expect(
        artifacts.result.inlineTokens.where(
          (token) => token.kind == SovereignMarkdownInlineKind.link,
        ),
        isEmpty,
      );
      expect(
        artifacts.result.hiddenRanges.where(
          (range) =>
              range.kind ==
              SovereignMarkdownHiddenRangeKind.referenceDefinition,
        ),
        isEmpty,
      );
    },
  ),
  _TransitionCase(
    id: 'gfm_bare_url_autolink',
    markdown: 'https://example.com',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    inlineTypes: const {'link'},
    overlayKinds: const {'link'},
    displayContains: const {'https://example.com'},
  ),
  _TransitionCase(
    id: 'gfm_bare_url_autolink_trims_trailing_punctuation',
    markdown: 'Visit www.commonmark.org.',
    profile: SovereignMarkdownProfile.commonMarkGfm,
    inlineTypes: const {'link'},
    overlayKinds: const {'link'},
    displayContains: const {'Visit www.commonmark.org.'},
    verify: (artifacts) {
      final target = artifacts.renderPlan.overlayPlan().targets.single;
      expect(target.sourceRange, const SovereignSourceRange(6, 24));
      expect(target.action?.destination, 'http://www.commonmark.org');
    },
  ),
  _TransitionCase(
    id: 'image_partial_destination_open',
    markdown: '![alt](',
    displayContains: const {'![alt]('},
    overlayKinds: const {},
  ),
  _TransitionCase(
    id: 'image_partial_destination_text_stays_literal',
    markdown: '![alt](url',
    blockTypes: const {'paragraph'},
    displayContains: const {'![alt](url'},
    overlayKinds: const {},
    verify: _expectNoInlineOrHiddenRanges,
  ),
  _TransitionCase(
    id: 'image_complete',
    markdown: '![alt](asset://image.png)',
    inlineTypes: const {'image'},
    hiddenTypes: const {'inlineMarker', 'linkDestination'},
    overlayKinds: const {'image'},
    displayContains: const {'alt'},
    displayOmits: const {'![', 'asset://image.png'},
  ),
  _TransitionCase(
    id: 'html_entity_partial',
    markdown: '&am',
    displayContains: const {'&am'},
    verify: (artifacts) => expect(artifacts.result.replacementRanges, isEmpty),
  ),
  _TransitionCase(
    id: 'html_entity_complete',
    markdown: '&amp;',
    replacementTypes: const {'htmlEntity'},
    displayContains: const {'&'},
    displayOmits: const {'&amp;'},
  ),
  _TransitionCase(
    id: 'raw_html_partial_open',
    markdown: '<span',
    displayContains: const {'<span'},
  ),
  _TransitionCase(
    id: 'raw_html_complete_inline',
    markdown: '<span>x</span>',
    inlineTypes: const {'htmlInline'},
    hiddenTypes: const {'rawHtml'},
    displayContains: const {'x'},
    displayOmits: const {'<span>', '</span>'},
  ),
];

void _expectNoHiddenRanges(_TransitionArtifacts artifacts) {
  expect(artifacts.result.hiddenRanges, isEmpty);
}

void _expectNoInlineOrHiddenRanges(_TransitionArtifacts artifacts) {
  expect(artifacts.result.inlineTokens, isEmpty);
  expect(artifacts.result.hiddenRanges, isEmpty);
}

void _expectNoErrors(SovereignMarkdownParseResult result, String id) {
  expect(
    result.diagnostics.where((diagnostic) {
      return diagnostic.extensions['isError'] == true;
    }),
    isEmpty,
    reason: id,
  );
}

void _expectRangesValid(SovereignMarkdownParseResult result, int sourceLength) {
  for (final range in [
    for (final block in _allBlocks(result.blocks)) block.sourceRange,
    for (final token in result.inlineTokens) token.sourceRange,
    for (final hiddenRange in result.hiddenRanges) hiddenRange.sourceRange,
    for (final replacementRange in result.replacementRanges)
      replacementRange.sourceRange,
  ]) {
    expect(range.start, greaterThanOrEqualTo(0));
    expect(range.end, lessThanOrEqualTo(sourceLength));
    expect(range.start, lessThanOrEqualTo(range.end));
  }
}

Iterable<SovereignMarkdownBlockNode> _allBlocks(
  Iterable<SovereignMarkdownBlockNode> blocks,
) sync* {
  for (final block in blocks) {
    yield block;
    yield* _allBlocks(block.children);
  }
}

void _expectContainsAll(Set<String> actual, Set<String> expected, String id) {
  for (final value in expected) {
    expect(actual, contains(value), reason: id);
  }
}

Set<String> _blockTypes(SovereignMarkdownParseResult result) {
  return {for (final block in _allBlocks(result.blocks)) block.type};
}

Set<String> _inlineTypes(SovereignMarkdownParseResult result) {
  return {for (final token in result.inlineTokens) token.type};
}

Set<String> _hiddenTypes(SovereignMarkdownParseResult result) {
  return {for (final hiddenRange in result.hiddenRanges) hiddenRange.type};
}

Set<String> _replacementTypes(SovereignMarkdownParseResult result) {
  return {
    for (final replacementRange in result.replacementRanges)
      replacementRange.type,
  };
}

Set<String> _overlayKinds(SovereignRenderPlan renderPlan) {
  return {
    for (final target in renderPlan.overlayPlan().targets) target.kind.name,
  };
}

final class _TransitionCase {
  const _TransitionCase({
    required this.id,
    required this.markdown,
    this.profile = SovereignMarkdownProfile.commonMarkCore,
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
  final SovereignMarkdownProfile profile;
  final Set<String> blockTypes;
  final Set<String> inlineTypes;
  final Set<String> hiddenTypes;
  final Set<String> replacementTypes;
  final Set<String> overlayKinds;
  final Set<String> displayContains;
  final Set<String> displayOmits;
  final void Function(_TransitionArtifacts artifacts)? verify;
}

final class _TransitionArtifacts {
  const _TransitionArtifacts({
    required this.result,
    required this.displayText,
    required this.renderPlan,
  });

  final SovereignMarkdownParseResult result;
  final String displayText;
  final SovereignRenderPlan renderPlan;
}
