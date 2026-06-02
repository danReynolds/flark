import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/src/v2/core/core.dart';
import 'package:sovereign_editor/src/v2/markdown/markdown.dart';

void main() {
  group('SovereignMarkdownParseResult', () {
    test('decodes parser payloads and preserves unknown fields', () {
      final result = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': SovereignMarkdownParseProtocol.currentSchemaVersion,
        'revision': 4,
        'sourceTextLength': 12,
        'blocks': [
          {
            'type': 'heading',
            'sourceRange': {'start': 0, 'end': 7},
            'attributes': {'level': 2},
            'futureBlockField': true,
          },
        ],
        'inlineTokens': [
          {
            'type': 'strong',
            'sourceRange': {'start': 8, 'end': 12},
            'attributes': {'marker': '**'},
            'futureInlineField': 42,
          },
        ],
        'hiddenRanges': [
          {
            'type': 'inlineMarker',
            'sourceRange': {'start': 8, 'end': 10},
            'attributes': {'marker': '**'},
            'futureHiddenField': 'kept',
          },
        ],
        'replacementRanges': [
          {
            'type': 'htmlEntity',
            'sourceRange': {'start': 10, 'end': 15},
            'replacementText': '&',
            'attributes': {'raw': '&amp;'},
            'futureReplacementField': 'kept',
          },
        ],
        'ambiguityZones': [
          {
            'type': 'delimiterRun',
            'sourceRange': {'start': 8, 'end': 12},
            'preferredAffinity': 'upstream',
            'attributes': {'candidate': '**'},
            'futureAmbiguityField': 'kept',
          },
        ],
        'diagnostics': [
          {
            'code': 'rawHtmlLiteral',
            'message': 'HTML is preserved as source text.',
            'sourceRange': {'start': 0, 'end': 4},
            'futureDiagnosticField': 'ok',
          },
        ],
        'futureResultField': 'ok',
      });

      expect(
        result.schemaVersion,
        SovereignMarkdownParseProtocol.currentSchemaVersion,
      );
      expect(result.revision, 4);
      expect(result.sourceTextLength, 12);
      expect(result.blocks.single.kind, SovereignMarkdownBlockKind.heading);
      expect(result.blocks.single.type, 'heading');
      expect(
          result.blocks.single.sourceRange, const SovereignSourceRange(0, 7));
      expect(result.blocks.single.attributes['level'], 2);
      expect(result.blocks.single.extensions['futureBlockField'], isTrue);
      expect(
          result.inlineTokens.single.kind, SovereignMarkdownInlineKind.strong);
      expect(
        result.inlineTokens.single.sourceRange,
        const SovereignSourceRange(8, 12),
      );
      expect(result.inlineTokens.single.extensions['futureInlineField'], 42);
      expect(
        result.hiddenRanges.single.kind,
        SovereignMarkdownHiddenRangeKind.inlineMarker,
      );
      expect(
        result.hiddenRanges.single.sourceRange,
        const SovereignSourceRange(8, 10),
      );
      expect(result.hiddenRanges.single.attributes['marker'], '**');
      expect(
          result.hiddenRanges.single.extensions['futureHiddenField'], 'kept');
      expect(
        result.replacementRanges.single.kind,
        SovereignMarkdownReplacementRangeKind.htmlEntity,
      );
      expect(
        result.replacementRanges.single.sourceRange,
        const SovereignSourceRange(10, 15),
      );
      expect(result.replacementRanges.single.replacementText, '&');
      expect(result.replacementRanges.single.attributes['raw'], '&amp;');
      expect(
        result.replacementRanges.single.extensions['futureReplacementField'],
        'kept',
      );
      expect(
        result.ambiguityZones.single.kind,
        SovereignMarkdownAmbiguityKind.delimiterRun,
      );
      expect(
        result.ambiguityZones.single.sourceRange,
        const SovereignSourceRange(8, 12),
      );
      expect(result.ambiguityZones.single.preferredAffinity,
          SovereignMapAffinity.upstream);
      expect(result.ambiguityZones.single.attributes['candidate'], '**');
      expect(
        result.ambiguityZones.single.extensions['futureAmbiguityField'],
        'kept',
      );
      expect(result.diagnostics.single.code, 'rawHtmlLiteral');
      expect(
        result.diagnostics.single.sourceRange,
        const SovereignSourceRange(0, 4),
      );
      expect(
        result.diagnostics.single.extensions['futureDiagnosticField'],
        'ok',
      );
      expect(result.extensions['futureResultField'], 'ok');
    });

    test('maps unknown block and inline variants without crashing', () {
      final result = SovereignMarkdownParseResult.fromJson({
        'schemaVersion': 99,
        'revision': 1,
        'sourceTextLength': 3,
        'blocks': [
          {
            'type': 'admonition',
            'sourceRange': {'start': 0, 'end': 3},
          },
        ],
        'inlineTokens': [
          {
            'type': 'wikilink',
            'sourceRange': {'start': 0, 'end': 3},
          },
        ],
        'hiddenRanges': [
          {
            'type': 'mathDelimiter',
            'sourceRange': {'start': 0, 'end': 1},
          },
        ],
        'replacementRanges': [
          {
            'type': 'futureReplacement',
            'sourceRange': {'start': 1, 'end': 2},
            'replacementText': 'x',
          },
        ],
        'ambiguityZones': [
          {
            'type': 'futureAmbiguity',
            'sourceRange': {'start': 1, 'end': 3},
            'preferredAffinity': 'sideways',
          },
        ],
      });

      expect(result.schemaVersion, 99);
      expect(result.blocks.single.kind, SovereignMarkdownBlockKind.unknown);
      expect(result.blocks.single.type, 'admonition');
      expect(
          result.inlineTokens.single.kind, SovereignMarkdownInlineKind.unknown);
      expect(result.inlineTokens.single.type, 'wikilink');
      expect(result.hiddenRanges.single.kind,
          SovereignMarkdownHiddenRangeKind.unknown);
      expect(result.hiddenRanges.single.type, 'mathDelimiter');
      expect(result.replacementRanges.single.kind,
          SovereignMarkdownReplacementRangeKind.unknown);
      expect(result.replacementRanges.single.type, 'futureReplacement');
      expect(result.ambiguityZones.single.kind,
          SovereignMarkdownAmbiguityKind.unknown);
      expect(result.ambiguityZones.single.type, 'futureAmbiguity');
      expect(result.ambiguityZones.single.preferredAffinity,
          SovereignMapAffinity.downstream);
    });
  });

  group('SovereignMarkdownParserCapabilities', () {
    test('reports supported profiles', () {
      final capabilities = SovereignMarkdownParserCapabilities(
        parserName: 'test',
        schemaVersion: SovereignMarkdownParseProtocol.currentSchemaVersion,
        supportedProfiles: const [
          SovereignMarkdownProfile.commonMarkCore,
        ],
      );

      expect(capabilities.supports(SovereignMarkdownProfile.commonMarkCore),
          isTrue);
      expect(capabilities.supports(SovereignMarkdownProfile.commonMarkGfm),
          isFalse);
    });
  });
}
