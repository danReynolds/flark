import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:sovereign_editor/sovereign_editor.dart';

class _TestSyntaxEngine implements SyntaxEngine {
  const _TestSyntaxEngine();

  @override
  Future<SyntaxSnapshot> parse(SyntaxParseRequest request) async {
    return SyntaxSnapshot(
      revision: request.revision,
      blocks: const <BlockSpan>[
        BlockSpan(
          type: BlockType.header,
          start: 0,
          end: 7,
          payload: <String, Object?>{'level': 1},
        ),
      ],
      inlineTokens: const <InlineSpanToken>[
        InlineSpanToken(style: SovereignStyle.bold, start: 2, end: 7),
      ],
      markerRanges: const <TextRange>[TextRange(start: 0, end: 2)],
      exclusionRanges: const <TextRange>[],
      ambiguityZones: const <TextRange>[],
      cursorMask: const PassthroughCursorValidationMask(textLength: 7),
      diagnostics: const <SyntaxDiagnostic>[],
    );
  }

  @override
  SyntaxPrediction predict(SyntaxPredictRequest request) {
    return SyntaxPrediction.empty(
      revision: request.revision,
      textLength: request.text.length,
    );
  }
}

void main() {
  testWidgets('top-level barrel exposes supported editor API', (tester) async {
    final controller = SovereignController(
      text: '# Title',
      syntaxEngine: const _TestSyntaxEngine(),
      markdownProfile: MarkdownSyntaxProfile.commonMarkGfm,
    );
    addTearDown(controller.dispose);

    final commands = controller.commands;
    expect(commands.capabilitiesAtSelection().canMutate, isTrue);

    const theme = SovereignEditorThemeData(
      cursorColor: Colors.blue,
      inlineText: SovereignInlineTextTheme(
        bold: TextStyle(fontWeight: FontWeight.w700),
      ),
    );

    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: Column(
            children: <Widget>[
              Expanded(
                child: SovereignEditor(
                  controller: controller,
                  theme: theme,
                  wrapText: true,
                ),
              ),
              const Expanded(
                child: SovereignMarkdownView(
                  markdown: '# Preview',
                  profile: MarkdownSyntaxProfile.commonMarkCore,
                  theme: theme,
                ),
              ),
            ],
          ),
        ),
      ),
    );

    expect(find.byType(SovereignEditor), findsOneWidget);
    expect(find.byType(SovereignMarkdownView), findsOneWidget);
  });

  test('top-level barrel exposes supported advanced API types', () {
    final mapper = Utf8Utf16OffsetMapper.fromText('a🙂b');
    expect(mapper.utf16ToUtf8(1), 1);

    const preflight = NativeComrakBridgePreflightResult.available();
    expect(preflight.isAvailable, isTrue);

    final theme = DuneMarkdownTheme.dune();
    expect(theme.headingScale(1), greaterThan(theme.headingScale(6)));
  });
}
