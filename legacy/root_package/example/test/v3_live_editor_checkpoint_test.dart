import 'package:example/v3_engine_lab.dart' show v3EngineLabWebAssets;
import 'package:example/v3_live_editor_checkpoint.dart';
import 'package:flark_flutter/flark_flutter_advanced.dart';
import 'package:flutter/foundation.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';

const _bareSchemeUri = 'https://example.test/a';
const _bareWwwUri = 'www.example.test/b';
const _bareEmail = 'me@example.test';

void main() {
  test('checkpoint seed exposes strict markerless GFM bare autolinks', () {
    expect(v3LiveCheckpointMarkdown, contains(_bareSchemeUri));
    expect(v3LiveCheckpointMarkdown, contains(_bareWwwUri));
    expect(v3LiveCheckpointMarkdown, contains(_bareEmail));
    expect(v3LiveCheckpointMarkdown, isNot(contains('<$_bareSchemeUri>')));
    expect(v3LiveCheckpointMarkdown, isNot(contains('<$_bareWwwUri>')));
    expect(v3LiveCheckpointMarkdown, isNot(contains('<$_bareEmail>')));
  });

  testWidgets('checkpoint shell promises marker-free live editing', (
    tester,
  ) async {
    await tester.pumpWidget(
      const V3LiveEditorCheckpointApp(openOnStart: false),
    );

    expect(find.text('Flark live editor'), findsOneWidget);
    expect(
      find.textContaining('syntax markers stay out of the editing surface'),
      findsOneWidget,
    );
    expect(find.textContaining('**'), findsNothing);
    expect(find.textContaining('```'), findsNothing);
  });

  testWidgets(
    'Web checkpoint reaches an exact parser-authored mixed viewport',
    (tester) async {
      final semantics = tester.ensureSemantics();
      final controller = FlarkV3VirtualizedLiveSurfaceController();
      FlarkV3DocumentRuntime? runtime;
      await tester.pumpWidget(
        V3LiveEditorCheckpointApp(
          webAssets: v3EngineLabWebAssets(flutterTestPackageServer: true),
          surfaceController: controller,
          onRuntimeOpened: (opened) => runtime = opened,
        ),
      );

      await _pumpUntil(
        tester,
        () =>
            controller.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
            _editableText().evaluate().length == 1,
        description: 'the real Worker/Wasm mixed-document viewport',
        debugState: () => 'snapshot=${controller.snapshot.runtimeType}',
      );

      final snapshot =
          controller.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
      final passiveParagraph = snapshot.blocks.firstWhere(
        (block) => block.displayText.contains('Write with bold'),
      );
      final passiveLinkParagraph = snapshot.blocks.firstWhere(
        (block) => block.displayText.contains('https://commonmark.org'),
      );
      final passiveDirectMediaParagraph = snapshot.blocks.firstWhere(
        (block) => block.displayText.contains('Flark architecture notes'),
      );
      final passiveReferenceMediaParagraph = snapshot.blocks.firstWhere(
        (block) => block.displayText.contains('Reference forms stay live too'),
      );
      final directLinkRuns = passiveDirectMediaParagraph.runs
          .where(
            (run) =>
                run.linkAnnotation?.destination ==
                'https://flark.dev/revision-7',
          )
          .toList(growable: false);
      final directImage = passiveDirectMediaParagraph.images.single;
      final referenceLinkRuns = passiveReferenceMediaParagraph.runs
          .where(
            (run) =>
                run.linkAnnotation?.kind == FlarkV3InlineLinkKind.reference,
          )
          .toList(growable: false);
      final referenceImage = passiveReferenceMediaParagraph.images.single;
      final cookedUriRuns = passiveLinkParagraph.runs
          .where(
            (run) => run.linkAnnotation?.destination == 'https://e.test/?q=&',
          )
          .toList(growable: false);
      final bareSchemeRuns = passiveLinkParagraph.runs
          .where((run) => run.linkAnnotation?.destination == _bareSchemeUri)
          .toList(growable: false);
      final bareWwwRuns = passiveLinkParagraph.runs
          .where(
            (run) => run.linkAnnotation?.destination == 'http://$_bareWwwUri',
          )
          .toList(growable: false);
      final bareEmailRuns = passiveLinkParagraph.runs
          .where(
            (run) => run.linkAnnotation?.destination == 'mailto:$_bareEmail',
          )
          .toList(growable: false);
      final passiveLayoutSentinel = find.byKey(
        ValueKey<Object>((
          'flark-v3-passive-text',
          passiveLinkParagraph.ordinal,
        )),
      );
      final display = snapshot.blocks
          .map((block) => block.displayText)
          .join('\n');
      expect(
        snapshot.blocks.map((block) => block.kind),
        containsAll([
          FlarkV3DocumentStructureKind.paragraph,
          FlarkV3DocumentStructureKind.heading,
          FlarkV3DocumentStructureKind.fencedCode,
        ]),
      );
      expect(display, contains('bold'));
      expect(display, contains('strikethrough'));
      expect(display, contains('escaped * punctuation'));
      expect(
        display,
        contains(
          'canonical Markdown exact.\n'
          'A parser-certified hard break',
        ),
      );
      expect(display, contains('https://commonmark.org'));
      expect(display, contains('hello@example.com'));
      expect(display, contains(_bareSchemeUri));
      expect(display, contains(_bareWwwUri));
      expect(display, contains(_bareEmail));
      expect(display, contains('©'));
      expect(display, contains('≧\u{338}'));
      expect(display, contains('https://e.test/?q=&'));
      expect(display, contains('Flark architecture notes'));
      expect(display, contains('Local architecture preview'));
      expect(display, contains('full reference'));
      expect(display, contains('collapsed reference'));
      expect(display, contains('shortcut reference'));
      expect(display, contains('Reference architecture'));
      expect(display, contains('A second idea'));
      expect(display, contains("final message = 'Hello from Flark';"));
      expect(display, isNot(contains('**')));
      expect(display, isNot(contains('_emphasis_')));
      expect(display, isNot(contains('`inline code`')));
      expect(display, isNot(contains('~~strikethrough~~')));
      expect(display, isNot(contains(r'\*')));
      expect(display, isNot(contains('canonical Markdown exact.  \n')));
      expect(display, isNot(contains('<https://commonmark.org>')));
      expect(display, isNot(contains('<hello@example.com>')));
      expect(display, isNot(contains('<$_bareSchemeUri>')));
      expect(display, isNot(contains('<$_bareWwwUri>')));
      expect(display, isNot(contains('<$_bareEmail>')));
      expect(display, isNot(contains('&copy;')));
      expect(display, isNot(contains('&ngE;')));
      expect(display, isNot(contains('&amp;')));
      expect(display, isNot(contains('<https://e.test/?q=&amp;>')));
      expect(display, isNot(contains('[Flark architecture notes]')));
      expect(display, isNot(contains('https://flark.dev/revision-7')));
      expect(display, isNot(contains('![Local architecture preview]')));
      expect(display, isNot(contains('asset://checkpoint/architecture')));
      expect(display, isNot(contains('[full reference][launch notes]')));
      expect(display, isNot(contains('[collapsed reference][]')));
      expect(display, isNot(contains('[shortcut reference]')));
      expect(display, isNot(contains('![Reference architecture]')));
      expect(display, isNot(contains('https://flark.dev/launch')));
      expect(display, isNot(contains('https://flark.dev/collapsed')));
      expect(display, isNot(contains('asset://checkpoint/reference')));
      expect(display, isNot(contains('```')));
      expect(_editableText(), findsOneWidget);
      expect(passiveLayoutSentinel, findsOneWidget);
      final passiveParagraphLeft = tester.getTopLeft(passiveLayoutSentinel).dx;
      expect(
        tester.getTopLeft(_editableText()).dx,
        closeTo(passiveParagraphLeft, 0.5),
        reason:
            'activating a passive block must not shift its text horizontally',
      );
      expect(find.text('Live editor ready'), findsOneWidget);
      expect(
        cookedUriRuns
            .map(
              (run) => passiveLinkParagraph.displayText.substring(
                run.startUtf16,
                run.endUtf16,
              ),
            )
            .join(),
        'https://e.test/?q=&',
      );
      expect(
        cookedUriRuns.map((run) => run.linkAnnotation!.destination),
        everyElement('https://e.test/?q=&'),
      );
      expect(
        bareSchemeRuns
            .map(
              (run) => passiveLinkParagraph.displayText.substring(
                run.startUtf16,
                run.endUtf16,
              ),
            )
            .join(),
        _bareSchemeUri,
      );
      expect(bareSchemeRuns, hasLength(1));
      expect(
        bareSchemeRuns.single.linkAnnotation!.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.exactContent,
      );
      expect(
        bareWwwRuns
            .map(
              (run) => passiveLinkParagraph.displayText.substring(
                run.startUtf16,
                run.endUtf16,
              ),
            )
            .join(),
        _bareWwwUri,
      );
      expect(bareWwwRuns, hasLength(1));
      expect(
        bareWwwRuns.single.linkAnnotation!.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.httpPrefixedExactContent,
      );
      expect(
        bareEmailRuns
            .map(
              (run) => passiveLinkParagraph.displayText.substring(
                run.startUtf16,
                run.endUtf16,
              ),
            )
            .join(),
        _bareEmail,
      );
      expect(bareEmailRuns, hasLength(1));
      expect(
        bareEmailRuns.single.linkAnnotation!.targetRecipe,
        FlarkV3InlineLinkTargetRecipe.mailtoExactContent,
      );
      expect(
        directLinkRuns
            .map(
              (run) => passiveDirectMediaParagraph.displayText.substring(
                run.startUtf16,
                run.endUtf16,
              ),
            )
            .join(),
        'Flark architecture notes',
      );
      expect(directLinkRuns, hasLength(1));
      expect(
        directLinkRuns.single.linkAnnotation!.kind,
        FlarkV3InlineLinkKind.direct,
      );
      expect(directLinkRuns.single.linkAnnotation!.title, 'Revision 7');
      expect(
        directImage.annotation.destination,
        'asset://checkpoint/architecture',
      );
      expect(directImage.annotation.title, 'Placeholder only');
      expect(
        passiveDirectMediaParagraph.displayText.substring(
          directImage.startUtf16,
          directImage.endUtf16,
        ),
        'Local architecture preview',
      );
      expect(referenceLinkRuns, hasLength(3));
      expect(
        referenceLinkRuns.map((run) => run.linkAnnotation!.destination).toSet(),
        {
          'https://flark.dev/launch',
          'https://flark.dev/collapsed',
          'https://commonmark.org',
        },
      );
      expect(
        referenceLinkRuns.map((run) => run.linkAnnotation!.targetRecipe),
        everyElement(FlarkV3InlineLinkTargetRecipe.companionCookedValue),
      );
      expect(
        referenceImage.annotation.destination,
        'asset://checkpoint/reference',
      );
      expect(referenceImage.annotation.title, 'Reference image');
      expect(
        passiveReferenceMediaParagraph.displayText.substring(
          referenceImage.startUtf16,
          referenceImage.endUtf16,
        ),
        'Reference architecture',
      );
      expect(
        find.byKey(const ValueKey<String>('flark-v3-inline-image-fallback')),
        findsOneWidget,
      );
      expect(
        find.byType(Image),
        findsNothing,
        reason: 'the checkpoint must not implicitly fetch image destinations',
      );

      expect(find.semantics.byLabel('https://commonmark.org'), findsOne);
      expect(find.semantics.byLabel('https://e.test/?q=&'), findsOne);
      expect(find.semantics.byLabel(_bareSchemeUri), findsOne);
      expect(find.semantics.byLabel(_bareWwwUri), findsOne);
      expect(find.semantics.byLabel(_bareEmail), findsOne);
      expect(find.semantics.byLabel('Flark architecture notes'), findsOne);
      expect(find.semantics.byLabel('Local architecture preview'), findsOne);
      expect(find.semantics.byLabel('full reference'), findsOne);
      expect(find.semantics.byLabel('collapsed reference'), findsOne);
      expect(find.semantics.byLabel('shortcut reference'), findsOne);
      expect(find.semantics.byLabel('Reference architecture'), findsOne);

      controller.revealAndActivateOrdinal(passiveParagraph.ordinal);
      await _pumpUntil(
        tester,
        () =>
            controller.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
            (controller.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
                    .activeOrdinal ==
                passiveParagraph.ordinal &&
            tester.widget<EditableText>(_editableText()).controller.text ==
                passiveParagraph.displayText,
        description:
            'the passive paragraph to become the certified live editor',
      );
      expect(
        tester
            .widget<Opacity>(
              find.byKey(const Key('flark-v3-active-visual-gate')),
            )
            .opacity,
        1,
      );
      expect(
        tester.getTopLeft(_editableText()).dx,
        closeTo(passiveParagraphLeft, 0.5),
        reason:
            'the live overlay must retain the passive row horizontal origin',
      );
      final activeParagraph = tester.widget<EditableText>(_editableText());
      expect(
        activeParagraph.controller.text,
        contains('escaped * punctuation'),
      );
      expect(activeParagraph.controller.text, isNot(contains(r'\*')));
      expect(runtime!.exportMarkdown(), contains(r'\*'));
      expect(
        runtime!.exportMarkdown(),
        contains('canonical Markdown exact.  \n'),
      );
      expect(runtime!.exportMarkdown(), v3LiveCheckpointMarkdown);
      expect(runtime!.exportMarkdown(), contains('&copy;'));
      expect(runtime!.exportMarkdown(), contains('&ngE;'));
      expect(runtime!.exportMarkdown(), contains('<https://e.test/?q=&amp;>'));
      expect(
        runtime!.exportMarkdown(),
        contains(
          '[Flark architecture notes]'
          '(https://flark.dev/revision-7 "Revision 7")',
        ),
      );
      expect(
        runtime!.exportMarkdown(),
        contains(
          '![Local architecture preview]'
          '(asset://checkpoint/architecture "Placeholder only")',
        ),
      );

      await _editMarkerFreeDirectMedia(
        tester,
        surfaceController: controller,
        runtime: runtime!,
        directMediaOrdinal: passiveDirectMediaParagraph.ordinal,
      );
      await _editMarkerFreeAutolink(
        tester,
        surfaceController: controller,
        runtime: runtime!,
        linkBlock: passiveLinkParagraph,
      );

      semantics.dispose();
      await tester.pumpWidget(const SizedBox.shrink());
      await tester.pump();
      await _awaitRuntimeClose(tester, runtime!);
    },
    skip: !kIsWeb,
  );
}

Future<void> _editMarkerFreeAutolink(
  WidgetTester tester, {
  required FlarkV3VirtualizedLiveSurfaceController surfaceController,
  required FlarkV3DocumentRuntime runtime,
  required FlarkV3ParserAuthoredBlockPresentation linkBlock,
}) async {
  expect(linkBlock.displayText, contains('hello@example.com'));
  expect(linkBlock.displayText, contains(_bareSchemeUri));
  expect(linkBlock.displayText, contains(_bareWwwUri));
  expect(linkBlock.displayText, contains(_bareEmail));
  expect(linkBlock.displayText, contains('©'));
  expect(linkBlock.displayText, contains('≧\u{338}'));
  expect(linkBlock.displayText, contains('https://e.test/?q=&'));
  expect(linkBlock.displayText, isNot(contains('<https://commonmark.org>')));
  expect(linkBlock.displayText, isNot(contains('<hello@example.com>')));
  expect(linkBlock.displayText, isNot(contains('&copy;')));
  expect(linkBlock.displayText, isNot(contains('&ngE;')));
  expect(linkBlock.displayText, isNot(contains('&amp;')));

  expect(
    surfaceController.snapshot,
    isA<FlarkV3ExactViewportSurfaceSnapshot>().having(
      (snapshot) => snapshot.activeOrdinal,
      'activeOrdinal',
      linkBlock.ordinal,
    ),
    reason:
        'the preceding direct-media handoff deliberately leaves this block '
        'active so repeated activation proves idempotent',
  );
  surfaceController.revealAndActivateOrdinal(linkBlock.ordinal);
  await tester.pump();
  await _pumpUntil(
    tester,
    () =>
        surfaceController.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
        (surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
                .activeOrdinal ==
            linkBlock.ordinal &&
        tester.widget<EditableText>(_editableText()).controller.text ==
            linkBlock.displayText,
    description: 'the marker-free autolink paragraph to become active',
  );

  final editableStateBefore = tester.state<EditableTextState>(_editableText());
  final editingController = tester
      .widget<EditableText>(_editableText())
      .controller;
  expect(editingController.text, contains('https://commonmark.org'));
  expect(editingController.text, contains('hello@example.com'));
  expect(editingController.text, contains(_bareSchemeUri));
  expect(editingController.text, contains(_bareWwwUri));
  expect(editingController.text, contains(_bareEmail));
  expect(editingController.text, contains('©'));
  expect(editingController.text, contains('≧\u{338}'));
  expect(editingController.text, contains('https://e.test/?q=&'));
  expect(editingController.text, isNot(contains('<')));
  expect(editingController.text, isNot(contains('>')));
  expect(editingController.text, isNot(contains('&copy;')));
  expect(editingController.text, isNot(contains('&ngE;')));
  expect(editingController.text, isNot(contains('&amp;')));

  editableStateBefore.requestKeyboard();
  await tester.pump();
  final setClientCallsBefore = tester.testTextInput.log
      .where((call) => call.method == 'TextInput.setClient')
      .toList(growable: false);
  final clientId =
      (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
  final exactBeforeEdit =
      surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
  final passiveSentinel = exactBeforeEdit.blocks.firstWhere(
    (block) =>
        block.ordinal != linkBlock.ordinal &&
        find
                .byKey(
                  ValueKey<Object>(('flark-v3-passive-text', block.ordinal)),
                )
                .evaluate()
                .length ==
            1,
  );
  final passiveRow = find.byKey(
    ValueKey<Object>(('flark-v3-passive-text', passiveSentinel.ordinal)),
  );
  const surface = Key('v3-live-checkpoint-surface');
  final surfaceFinder = find.byKey(surface);
  expect(passiveRow, findsOneWidget);
  expect(surfaceFinder, findsOneWidget);
  final passiveRenderBefore = tester.renderObject<RenderParagraph>(passiveRow);
  final passiveRectBefore = tester.getRect(passiveRow);
  final surfaceRectBefore = tester.getRect(surfaceFinder);
  final statusBefore = tester.widget<Text>(
    find.byKey(const Key('v3-live-checkpoint-status')),
  );
  expect(passiveRenderBefore.attached, isTrue);
  expect(statusBefore.data, 'Live editor ready');
  expect(find.byKey(const Key('v3-live-checkpoint-diagnostic')), findsNothing);
  final revisionBefore = runtime.sourceRevision;
  final sourceBefore = runtime.exportMarkdown();
  final bareSchemeStart = editingController.text.indexOf(_bareSchemeUri);
  expect(bareSchemeStart, greaterThanOrEqualTo(0));
  final destinationStart = bareSchemeStart + _bareSchemeUri.length - 1;
  expect(editingController.text[destinationStart], 'a');

  (editableStateBefore as DeltaTextInputClient).updateEditingValueWithDeltas([
    TextEditingDeltaReplacement(
      oldText: editingController.text,
      replacedRange: TextRange(
        start: destinationStart,
        end: destinationStart + 1,
      ),
      replacementText: 'live',
      selection: TextSelection.collapsed(
        offset: destinationStart + 'live'.length,
      ),
      composing: TextRange.empty,
    ),
  ]);
  await tester.pump();

  expect(
    passiveRow,
    findsOneWidget,
    reason: 'an unrelated certified row must not blink during recertification',
  );
  expect(
    tester.renderObject<RenderParagraph>(passiveRow),
    same(passiveRenderBefore),
    reason: 'the retained passive row must keep its render identity',
  );
  expect(passiveRenderBefore.attached, isTrue);
  expect(tester.getRect(passiveRow), passiveRectBefore);
  expect(tester.getRect(surfaceFinder), surfaceRectBefore);
  expect(
    tester.state<EditableTextState>(_editableText()),
    same(editableStateBefore),
  );
  expect(
    tester
        .widget<Text>(find.byKey(const Key('v3-live-checkpoint-status')))
        .data,
    statusBefore.data,
  );
  expect(find.text('Starting runtime'), findsNothing);
  expect(find.byKey(const Key('v3-live-checkpoint-diagnostic')), findsNothing);

  const updatedDestination = 'https://example.test/live';
  final expectedSource = sourceBefore.replaceFirst(
    _bareSchemeUri,
    updatedDestination,
  );
  expect(editingController.text, contains(updatedDestination));
  expect(editingController.text, contains('https://commonmark.org'));
  expect(editingController.text, contains('hello@example.com'));
  expect(editingController.text, contains(_bareWwwUri));
  expect(editingController.text, contains(_bareEmail));
  expect(editingController.text, contains('©'));
  expect(editingController.text, contains('≧\u{338}'));
  expect(editingController.text, contains('https://e.test/?q=&'));
  expect(editingController.text, isNot(contains('<')));
  expect(editingController.text, isNot(contains('>')));
  expect(runtime.exportMarkdown(), expectedSource);
  expect(runtime.sourceRevision, revisionBefore + 1);

  await _pumpUntil(
    tester,
    () {
      final snapshot = surfaceController.snapshot;
      if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot ||
          snapshot.activeOrdinal != linkBlock.ordinal ||
          !runtime.status.sourceCurrent ||
          !runtime.status.structureCurrent) {
        return false;
      }
      final updatedBlock = snapshot.blocks.firstWhere(
        (block) => block.ordinal == linkBlock.ordinal,
      );
      final destinations = updatedBlock.runs
          .map((run) => run.linkAnnotation?.destination)
          .whereType<String>()
          .toSet();
      final projected = editingController;
      return updatedBlock.displayText.contains(updatedDestination) &&
          destinations.contains(updatedDestination) &&
          destinations.contains('https://commonmark.org') &&
          destinations.contains('http://$_bareWwwUri') &&
          destinations.contains('mailto:hello@example.com') &&
          destinations.contains('mailto:$_bareEmail') &&
          destinations.contains('https://e.test/?q=&') &&
          projected is FlarkV3InlineTextEditingController &&
          projected.hasCertifiedPresentation;
    },
    description: 'the edited URI to receive exact parser recertification',
    debugState: () =>
        'revision=${runtime.status.sourceRevision}; '
        'sourceCurrent=${runtime.status.sourceCurrent}; '
        'structureCurrent=${runtime.status.structureCurrent}',
  );

  expect(
    tester.state<EditableTextState>(_editableText()),
    same(editableStateBefore),
  );
  final setClientCallsAfter = tester.testTextInput.log
      .where((call) => call.method == 'TextInput.setClient')
      .toList(growable: false);
  expect(setClientCallsAfter, hasLength(setClientCallsBefore.length));
  expect((setClientCallsAfter.last.arguments as List<dynamic>).first, clientId);
  expect(runtime.exportMarkdown(), expectedSource);

  final recertifiedSnapshot =
      surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
  final otherBlock = recertifiedSnapshot.blocks.firstWhere(
    (block) =>
        block.ordinal != linkBlock.ordinal &&
        block.kind == FlarkV3DocumentStructureKind.paragraph,
  );
  surfaceController.revealAndActivateOrdinal(otherBlock.ordinal);
  await _pumpUntil(
    tester,
    () =>
        surfaceController.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
        (surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
                .activeOrdinal ==
            otherBlock.ordinal &&
        find.semantics.byLabel(updatedDestination).evaluate().length == 1,
    description: 'the edited link paragraph to become passive',
  );

  expect(find.semantics.byLabel(_bareSchemeUri), findsNothing);
  expect(find.semantics.byLabel(updatedDestination), findsOne);
  expect(find.semantics.byLabel('https://commonmark.org'), findsOne);
  final passiveUpdatedBlock =
      (surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot).blocks
          .firstWhere((block) => block.ordinal == linkBlock.ordinal);
  await tester.tapAt(
    _passiveTextRangeCenter(
      tester,
      ordinal: passiveUpdatedBlock.ordinal,
      displayText: passiveUpdatedBlock.displayText,
      target: updatedDestination,
    ),
  );
  await _pumpUntil(
    tester,
    () =>
        find
            .text('Parser-certified destination: $updatedDestination')
            .evaluate()
            .length ==
        1,
    timeout: const Duration(seconds: 3),
    description: 'the updated parser-certified link callback',
  );
  expect(
    find.text('Parser-certified destination: $updatedDestination'),
    findsOneWidget,
  );
}

Future<void> _editMarkerFreeDirectMedia(
  WidgetTester tester, {
  required FlarkV3VirtualizedLiveSurfaceController surfaceController,
  required FlarkV3DocumentRuntime runtime,
  required int directMediaOrdinal,
}) async {
  const originalLinkLabel = 'Flark architecture notes';
  const replacedLinkLabel = 'Flark design notes';
  const finalLinkLabel = 'Flark design notes today';
  const originalImageAlt = 'Local architecture preview';
  const finalImageAlt = 'Live parser preview';
  const destination = 'https://flark.dev/revision-7';
  const linkTitle = 'Revision 7';
  const imageDestination = 'asset://checkpoint/architecture';
  const imageTitle = 'Placeholder only';
  const originalLinkSource = '[$originalLinkLabel]($destination "$linkTitle")';
  const replacedLinkSource = '[$replacedLinkLabel]($destination "$linkTitle")';
  const finalLinkSource = '[$finalLinkLabel]($destination "$linkTitle")';
  const originalImageSource =
      '![$originalImageAlt]($imageDestination "$imageTitle")';
  const finalImageSource = '![$finalImageAlt]($imageDestination "$imageTitle")';

  final beforeActivation =
      surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
  final directMediaBefore = beforeActivation.blocks.singleWhere(
    (block) => block.ordinal == directMediaOrdinal,
  );
  surfaceController.revealAndActivateOrdinal(directMediaOrdinal);
  await _pumpUntil(
    tester,
    () =>
        surfaceController.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
        (surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
                .activeOrdinal ==
            directMediaOrdinal &&
        tester.widget<EditableText>(_editableText()).controller.text ==
            directMediaBefore.displayText,
    description: 'the direct-media paragraph to become the live editor',
    debugState: () =>
        'runtime=${runtime.status.state.name}; '
        'snapshot=${surfaceController.snapshot.runtimeType}; '
        'active=${surfaceController.snapshot?.activeOrdinal}; '
        'wanted=$directMediaOrdinal; '
        'inline=${runtime.status.inlinePresentationGeneration}/'
        '${runtime.status.inlineAttemptOutcomeGeneration}',
  );

  final editableState = tester.state<EditableTextState>(_editableText());
  final deltaClient = editableState as DeltaTextInputClient;
  final editingController = tester
      .widget<EditableText>(_editableText())
      .controller;
  expect(editingController.text, contains(originalLinkLabel));
  expect(editingController.text, contains(originalImageAlt));
  expect(editingController.text, isNot(contains(destination)));
  expect(editingController.text, isNot(contains(linkTitle)));
  expect(editingController.text, isNot(contains(imageDestination)));
  expect(editingController.text, isNot(contains(imageTitle)));

  editableState.requestKeyboard();
  await tester.pump();
  final setClientCallsBefore = tester.testTextInput.log
      .where((call) => call.method == 'TextInput.setClient')
      .toList(growable: false);
  final clientId =
      (setClientCallsBefore.last.arguments as List<dynamic>).first as int;
  final sourceBefore = runtime.exportMarkdown();
  expect(sourceBefore, contains(originalLinkSource));
  expect(sourceBefore, contains(originalImageSource));

  final firstRevision = runtime.sourceRevision;
  final originalDisplay = editingController.text;
  final originalLabelStart = originalDisplay.indexOf(originalLinkLabel);
  expect(originalLabelStart, greaterThanOrEqualTo(0));
  deltaClient.updateEditingValueWithDeltas([
    TextEditingDeltaReplacement(
      oldText: originalDisplay,
      replacedRange: TextRange(
        start: originalLabelStart,
        end: originalLabelStart + originalLinkLabel.length,
      ),
      replacementText: replacedLinkLabel,
      selection: TextSelection.collapsed(
        offset: originalLabelStart + replacedLinkLabel.length,
      ),
      composing: TextRange.empty,
    ),
  ]);
  await tester.pump();

  final sourceAfterReplacement = sourceBefore.replaceFirst(
    originalLinkSource,
    replacedLinkSource,
  );
  expect(editingController.text, contains(replacedLinkLabel));
  expect(editingController.text, isNot(contains(originalLinkLabel)));
  expect(runtime.exportMarkdown(), sourceAfterReplacement);
  expect(runtime.sourceRevision, firstRevision + 1);
  await _awaitDirectMediaCertification(
    tester,
    surfaceController: surfaceController,
    runtime: runtime,
    ordinal: directMediaOrdinal,
    expectedRevision: firstRevision + 1,
    linkLabel: replacedLinkLabel,
    imageAlt: originalImageAlt,
    destination: destination,
    linkTitle: linkTitle,
    imageDestination: imageDestination,
    imageTitle: imageTitle,
  );

  final secondRevision = runtime.sourceRevision;
  final displayBeforeBoundaryInsertion = editingController.text;
  final finalLabelBoundary =
      displayBeforeBoundaryInsertion.indexOf(replacedLinkLabel) +
      replacedLinkLabel.length;
  expect(
    displayBeforeBoundaryInsertion.substring(
      finalLabelBoundary,
      finalLabelBoundary + 1,
    ),
    ' ',
    reason: 'the insertion point must be the final visible link-label boundary',
  );
  deltaClient.updateEditingValueWithDeltas([
    TextEditingDeltaInsertion(
      oldText: displayBeforeBoundaryInsertion,
      textInserted: ' today',
      insertionOffset: finalLabelBoundary,
      selection: TextSelection.collapsed(
        offset: finalLabelBoundary + ' today'.length,
      ),
      composing: TextRange.empty,
    ),
  ]);
  await tester.pump();

  final sourceAfterBoundaryInsertion = sourceAfterReplacement.replaceFirst(
    replacedLinkSource,
    finalLinkSource,
  );
  expect(editingController.text, contains(finalLinkLabel));
  expect(runtime.exportMarkdown(), sourceAfterBoundaryInsertion);
  expect(
    runtime.exportMarkdown(),
    isNot(contains('$replacedLinkSource today')),
    reason: 'the boundary insertion must stay before the hidden direct tail',
  );
  expect(runtime.sourceRevision, secondRevision + 1);
  await _awaitDirectMediaCertification(
    tester,
    surfaceController: surfaceController,
    runtime: runtime,
    ordinal: directMediaOrdinal,
    expectedRevision: secondRevision + 1,
    linkLabel: finalLinkLabel,
    imageAlt: originalImageAlt,
    destination: destination,
    linkTitle: linkTitle,
    imageDestination: imageDestination,
    imageTitle: imageTitle,
  );

  final thirdRevision = runtime.sourceRevision;
  final displayBeforeImageEdit = editingController.text;
  final imageAltStart = displayBeforeImageEdit.indexOf(originalImageAlt);
  expect(imageAltStart, greaterThanOrEqualTo(0));
  deltaClient.updateEditingValueWithDeltas([
    TextEditingDeltaReplacement(
      oldText: displayBeforeImageEdit,
      replacedRange: TextRange(
        start: imageAltStart,
        end: imageAltStart + originalImageAlt.length,
      ),
      replacementText: finalImageAlt,
      selection: TextSelection.collapsed(
        offset: imageAltStart + finalImageAlt.length,
      ),
      composing: TextRange.empty,
    ),
  ]);
  await tester.pump();

  final finalSource = sourceAfterBoundaryInsertion.replaceFirst(
    originalImageSource,
    finalImageSource,
  );
  expect(editingController.text, contains(finalLinkLabel));
  expect(editingController.text, contains(finalImageAlt));
  expect(editingController.text, isNot(contains(originalImageAlt)));
  expect(editingController.text, isNot(contains(destination)));
  expect(editingController.text, isNot(contains(linkTitle)));
  expect(editingController.text, isNot(contains(imageDestination)));
  expect(editingController.text, isNot(contains(imageTitle)));
  expect(runtime.exportMarkdown(), finalSource);
  expect(runtime.sourceRevision, thirdRevision + 1);
  await _awaitDirectMediaCertification(
    tester,
    surfaceController: surfaceController,
    runtime: runtime,
    ordinal: directMediaOrdinal,
    expectedRevision: thirdRevision + 1,
    linkLabel: finalLinkLabel,
    imageAlt: finalImageAlt,
    destination: destination,
    linkTitle: linkTitle,
    imageDestination: imageDestination,
    imageTitle: imageTitle,
  );

  expect(tester.state<EditableTextState>(_editableText()), same(editableState));
  final setClientCallsAfterEdits = tester.testTextInput.log
      .where((call) => call.method == 'TextInput.setClient')
      .toList(growable: false);
  expect(setClientCallsAfterEdits, hasLength(setClientCallsBefore.length));
  expect(
    (setClientCallsAfterEdits.last.arguments as List<dynamic>).first,
    clientId,
  );

  final recertified =
      surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
  final neighboringBlocks =
      recertified.blocks
          .where(
            (block) =>
                block.ordinal != directMediaOrdinal &&
                block.isAuthoritative &&
                block.displayText.isNotEmpty,
          )
          .toList(growable: false)
        ..sort(
          (left, right) => (left.ordinal - directMediaOrdinal).abs().compareTo(
            (right.ordinal - directMediaOrdinal).abs(),
          ),
        );
  final otherBlock = neighboringBlocks.first;
  surfaceController.revealAndActivateOrdinal(otherBlock.ordinal);
  final editedDirectImage = find.byWidgetPredicate(
    (widget) =>
        widget is FlarkV3InlineImage && widget.spec.alt == finalImageAlt,
    description: 'the recertified direct image',
  );
  final editedDirectImageFallback = find.descendant(
    of: editedDirectImage,
    matching: find.byKey(
      const ValueKey<String>('flark-v3-inline-image-fallback'),
    ),
  );
  await _pumpUntil(
    tester,
    () =>
        surfaceController.snapshot is FlarkV3ExactViewportSurfaceSnapshot &&
        (surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot)
                .activeOrdinal ==
            otherBlock.ordinal &&
        find.semantics.byLabel(finalLinkLabel).evaluate().length == 1 &&
        find.semantics.byLabel(finalImageAlt).evaluate().length == 1 &&
        editedDirectImageFallback.evaluate().length == 1,
    description: 'the edited direct media to become passive',
    timeout: const Duration(seconds: 3),
    debugState: () {
      final snapshot = surfaceController.snapshot;
      final block = switch (snapshot) {
        FlarkV3ExactViewportSurfaceSnapshot(:final blocks) =>
          blocks.singleWhere((block) => block.ordinal == directMediaOrdinal),
        _ => null,
      };
      return 'snapshot=${snapshot.runtimeType}; '
          'active=${snapshot?.activeOrdinal}; '
          'linkSemantics=${find.semantics.byLabel(finalLinkLabel).evaluate().length}; '
          'imageSemantics=${find.semantics.byLabel(finalImageAlt).evaluate().length}; '
          'fallback=${find.byKey(const ValueKey<String>('flark-v3-inline-image-fallback')).evaluate().length}; '
          'imageWidgets=${find.byType(FlarkV3InlineImage).evaluate().length}; '
          'editable=${tester.widget<EditableText>(_editableText()).controller.text}; '
          'display=${block?.displayText}; images=${block?.images.length}';
    },
  );

  expect(find.semantics.byLabel(originalLinkLabel), findsNothing);
  expect(find.semantics.byLabel(finalLinkLabel), findsOne);
  expect(find.semantics.byLabel(originalImageAlt), findsNothing);
  expect(find.semantics.byLabel(finalImageAlt), findsOne);
  expect(editedDirectImageFallback, findsOneWidget);
  expect(find.byType(Image), findsNothing);
  final passiveDirectMediaFinder = find.byKey(
    ValueKey<Object>(('flark-v3-passive-text', directMediaOrdinal)),
  );
  await tester.ensureVisible(passiveDirectMediaFinder);
  await tester.pump();
  final passiveSnapshot =
      surfaceController.snapshot as FlarkV3ExactViewportSurfaceSnapshot;
  final passiveDirectMedia = passiveSnapshot.blocks.singleWhere(
    (block) => block.ordinal == directMediaOrdinal,
  );
  await tester.tapAt(
    _passiveTextRangeCenter(
      tester,
      ordinal: directMediaOrdinal,
      displayText: passiveDirectMedia.displayText,
      target: finalLinkLabel,
    ),
  );
  await _pumpUntil(
    tester,
    () =>
        find
            .text('Parser-certified destination: $destination')
            .evaluate()
            .length ==
        1,
    timeout: const Duration(seconds: 3),
    description: 'the updated direct-link callback',
  );
  expect(
    find.text('Parser-certified destination: $destination'),
    findsOneWidget,
  );
  expect(tester.state<EditableTextState>(_editableText()), same(editableState));
  final finalSetClientCalls = tester.testTextInput.log
      .where((call) => call.method == 'TextInput.setClient')
      .toList(growable: false);
  expect(finalSetClientCalls, hasLength(setClientCallsBefore.length));
  expect((finalSetClientCalls.last.arguments as List<dynamic>).first, clientId);
  expect(runtime.exportMarkdown(), finalSource);
}

Future<void> _awaitDirectMediaCertification(
  WidgetTester tester, {
  required FlarkV3VirtualizedLiveSurfaceController surfaceController,
  required FlarkV3DocumentRuntime runtime,
  required int ordinal,
  required int expectedRevision,
  required String linkLabel,
  required String imageAlt,
  required String destination,
  required String linkTitle,
  required String imageDestination,
  required String imageTitle,
}) async {
  var certified = false;
  await _pumpUntil(
    tester,
    () {
      if (runtime.status.state == FlarkV3DocumentRuntimeState.closed) {
        return true;
      }
      final snapshot = surfaceController.snapshot;
      if (snapshot is! FlarkV3ExactViewportSurfaceSnapshot ||
          snapshot.activeOrdinal != ordinal ||
          runtime.sourceRevision != expectedRevision ||
          !runtime.status.sourceCurrent ||
          !runtime.status.structureCurrent) {
        return false;
      }
      final matching = snapshot.blocks
          .where((block) => block.ordinal == ordinal)
          .toList(growable: false);
      if (matching.length != 1) return false;
      final block = matching.single;
      final linkRuns = block.runs
          .where((run) => run.linkAnnotation?.destination == destination)
          .toList(growable: false);
      if (linkRuns.length != 1 ||
          linkRuns.single.linkAnnotation?.kind !=
              FlarkV3InlineLinkKind.direct ||
          linkRuns.single.linkAnnotation?.title != linkTitle ||
          block.images.length != 1) {
        return false;
      }
      final image = block.images.single;
      final controller = tester
          .widget<EditableText>(_editableText())
          .controller;
      certified =
          block.displayText.substring(
                linkRuns.single.startUtf16,
                linkRuns.single.endUtf16,
              ) ==
              linkLabel &&
          block.displayText.substring(image.startUtf16, image.endUtf16) ==
              imageAlt &&
          image.annotation.destination == imageDestination &&
          image.annotation.title == imageTitle &&
          image.outerLink == null &&
          controller is FlarkV3InlineTextEditingController &&
          controller.hasCertifiedPresentation;
      return certified;
    },
    description: 'the edited direct link and image to receive parser authority',
    debugState: () =>
        'revision=${runtime.status.sourceRevision}; '
        'sourceCurrent=${runtime.status.sourceCurrent}; '
        'structureCurrent=${runtime.status.structureCurrent}',
  );
  if (!certified) {
    await _awaitRuntimeClose(tester, runtime);
    fail('The runtime closed before direct-media certification completed.');
  }
}

Finder _editableText() => find.byWidgetPredicate(
  (widget) => widget is EditableText,
  description: 'the single live EditableText',
);

Offset _passiveTextRangeCenter(
  WidgetTester tester, {
  required int ordinal,
  required String displayText,
  required String target,
}) {
  final start = displayText.indexOf(target);
  if (start < 0) {
    throw StateError('Passive block $ordinal does not contain "$target".');
  }
  final paragraph = tester.renderObject<RenderParagraph>(
    find.byKey(ValueKey<Object>(('flark-v3-passive-text', ordinal))),
  );
  final boxes = paragraph.getBoxesForSelection(
    TextSelection(baseOffset: start, extentOffset: start + target.length),
  );
  if (boxes.isEmpty) {
    throw StateError('Passive target "$target" has no hit-test box.');
  }
  return paragraph.localToGlobal(boxes.first.toRect().center);
}

Future<void> _awaitRuntimeClose(
  WidgetTester tester,
  FlarkV3DocumentRuntime runtime,
) async {
  var complete = false;
  Object? closeError;
  StackTrace? closeStack;
  final observedClose = runtime.close().then<void>(
    (_) => complete = true,
    onError: (Object error, StackTrace stackTrace) {
      closeError = error;
      closeStack = stackTrace;
      debugPrint('V3_RUNTIME_TERMINAL_FAILURE: $error\n$stackTrace');
      complete = true;
    },
  );
  await _pumpUntil(
    tester,
    () => complete,
    timeout: const Duration(seconds: 10),
    description: 'the checkpoint endpoint removal receipt',
    debugState: () => 'runtime=${runtime.status.state.name}',
  );
  await observedClose;
  if (closeError != null) {
    Error.throwWithStackTrace(closeError!, closeStack!);
  }
}

Future<void> _pumpUntil(
  WidgetTester tester,
  bool Function() condition, {
  Duration timeout = const Duration(seconds: 20),
  required String description,
  String Function()? debugState,
}) async {
  final watch = Stopwatch()..start();
  while (!condition()) {
    final frameworkException = tester.takeException();
    if (frameworkException != null) {
      debugPrint(
        'V3_LIVE_CHECKPOINT_FRAMEWORK_EXCEPTION: $description; '
        '$frameworkException',
      );
      fail(
        'Framework exception while waiting for $description: '
        '$frameworkException',
      );
    }
    if (watch.elapsed >= timeout) {
      final visibleText = tester
          .widgetList<Text>(find.byType(Text))
          .map((widget) => widget.data)
          .whereType<String>()
          .where((value) => value.isNotEmpty)
          .join(' | ');
      debugPrint(
        'V3_LIVE_CHECKPOINT_TIMEOUT: $description; '
        '${debugState?.call() ?? ''}; visible text: $visibleText',
      );
      fail(
        'Timed out waiting for $description within $timeout. '
        'Visible text: $visibleText',
      );
    }
    await tester.runAsync(
      () => Future<void>.delayed(const Duration(milliseconds: 20)),
    );
    await tester.pump(const Duration(milliseconds: 20));
  }
}
