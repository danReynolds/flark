// Widget-level IME (input method editor) input tests.
//
// Every scenario drives real platform-shaped text input — [TextEditingValue]
// updates with composing ranges, delivered over the engine test text-input
// channel via `tester.testTextInput.updateEditingValue` — through the editing
// surface that [FlarkMarkdownEditor] actually mounts in live mode:
// [FlarkLiveRenderedEditableText]. For documents made of plain paragraphs and
// inline styling (most scenarios below) that widget mounts the whole-document
// projected host — one [EditableText] whose edits are classified by
// `classifyFlarkHostEdit`. Documents containing quotes/lists/code fences mount
// per-block editables classified by `classifyFlarkLiveBlockEdit`; the final
// group drives those.
//
// After every commit-level step the tests assert the package invariant from
// docs/architecture/v2/inline_delimiter_validity_2026-07-10.md:
//   (a) display fidelity — the projected display equals what the user
//       semantically typed, and
//   (b) export round-trip — a fresh, caret-free controller parsing
//       `controller.markdown` projects the identical display, proving the
//       source never depends on editor-local state.
// See [_expectCommitted].

import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flark/src/v2/core/core.dart';
import 'package:flark/src/v2/flutter/flutter.dart';
import 'package:flark/src/v2/markdown/markdown.dart';

void main() {
  group('host surface (whole-document projected editable)', () {
    testWidgets('predictive composition commits two plain words', (
      tester,
    ) async {
      // The GBoard shape: the composing region covers the word being typed
      // and the commit collapses composing while appending the trailing
      // space in the same update.
      final controller = await _pumpLiveEditor(tester, '');
      final editable = find.byType(EditableText);
      await tester.showKeyboard(editable);
      await tester.pump();

      await _composeAndCommit(
        tester,
        editable,
        regionStart: 0,
        stages: const ['h', 'he', 'hello'],
        commit: 'hello ',
      );
      await _expectCommitted(
        tester,
        controller,
        display: 'hello ',
        source: 'hello ',
      );

      await tester.showKeyboard(editable);
      await _composeAndCommit(
        tester,
        editable,
        regionStart: 6,
        stages: const ['w', 'wo', 'world'],
        commit: 'world ',
      );
      await _expectCommitted(
        tester,
        controller,
        display: 'hello world ',
        source: 'hello world ',
      );
      expect(_remoteValue(tester, editable).text, 'hello world ');
    });

    testWidgets(
      'predictive composition with strong armed commits the canonical '
      '**word** shape and re-enters on the next word',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, '');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        controller.commands.toggleInlineStyle(FlarkMarkdownInlineStyle.strong);
        await tester.pump();

        await _composeAndCommit(
          tester,
          editable,
          regionStart: 0,
          stages: const ['h', 'he', 'hello'],
          commit: 'hello ',
        );
        // Canonical form per the invariant doc: the closing delimiter never
        // hugs the trailing space (`**hello **` is unrepresentable); the
        // space commits outside and the style stays armed.
        await _expectCommitted(
          tester,
          controller,
          display: 'hello ',
          source: '**hello** ',
        );

        await tester.showKeyboard(editable);
        await _composeAndCommit(
          tester,
          editable,
          regionStart: 6,
          stages: const ['w', 'wo', 'world'],
          commit: 'world ',
        );
        // The armed continuation re-enters the run: one styled run, not two
        // siblings.
        await _expectCommitted(
          tester,
          controller,
          display: 'hello world ',
          source: '**hello world** ',
        );
      },
      // Historical defect, now fixed: the armed wrap writes '**h**', and the
      // placement reports its authored delimiter ranges so the controller
      // hides them in the *predicted* projection on the same frame — the
      // editable never resyncs raw markers to the platform, so the composing
      // region survives every marker-creating/relocating keystroke (fresh
      // wraps, re-entry relocations, edge repairs). The immediate parse then
      // re-derives the identical hidden ranges authoritatively.
    );

    testWidgets(
      'predictive composition with strong armed reaches the canonical '
      '**word** shape (outcome only)',
      (tester) async {
        // Outcome-level companion to the strict test above: every committed
        // document state must land on the canonical invariant-doc shape
        // (composing preservation is the strict test's contract).
        final controller = await _pumpLiveEditor(tester, '');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        controller.commands.toggleInlineStyle(FlarkMarkdownInlineStyle.strong);
        await tester.pump();

        await _composeAndCommit(
          tester,
          editable,
          regionStart: 0,
          stages: const ['h', 'he', 'hello'],
          commit: 'hello ',
        );
        await _expectCommitted(
          tester,
          controller,
          display: 'hello ',
          source: '**hello** ',
        );

        await tester.showKeyboard(editable);
        await _composeAndCommit(
          tester,
          editable,
          regionStart: 6,
          stages: const ['w', 'wo', 'world'],
          commit: 'world ',
        );
        // The armed continuation re-enters the run: one styled run, not two
        // siblings.
        await _expectCommitted(
          tester,
          controller,
          display: 'hello world ',
          source: '**hello world** ',
        );
      },
    );

    testWidgets(
      'autocorrect replaces the word behind the caret in one committed update',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, '');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();

        for (final typed in const ['t', 'te', 'teh']) {
          await _sendIme(
            tester,
            TextEditingValue(
              text: typed,
              selection: TextSelection.collapsed(offset: typed.length),
            ),
          );
        }
        await _expectCommitted(
          tester,
          controller,
          display: 'teh',
          source: 'teh',
        );

        // On the space keystroke the platform rewrites the finished word
        // behind the caret — offsets 0..3 'teh' -> 'the' — and appends the
        // space in the same update, with no composing range (the iOS
        // autocorrect shape).
        await tester.showKeyboard(editable);
        await _sendIme(
          tester,
          const TextEditingValue(
            text: 'the ',
            selection: TextSelection.collapsed(offset: 4),
          ),
        );
        await _expectCommitted(
          tester,
          controller,
          display: 'the ',
          source: 'the ',
        );
      },
    );

    testWidgets(
      'Japanese conversion rewrites the composing region wholesale in '
      'plain text',
      (tester) async {
        // Romaji -> kana -> kanji: the region converts wholesale and its
        // length changes across stages ('かん' is two units, '感' one).
        final controller = await _pumpLiveEditor(tester, '');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();

        await _composeAndCommit(
          tester,
          editable,
          regionStart: 0,
          stages: const ['k', 'か', 'かん', '感'],
          commit: '感',
        );
        await _expectCommitted(tester, controller, display: '感', source: '感');
      },
    );

    testWidgets('Japanese conversion composes inside an existing strong run', (
      tester,
    ) async {
      final controller = await _pumpLiveEditor(tester, '**bold**');
      final editable = find.byType(EditableText);
      await tester.showKeyboard(editable);
      await tester.pump();
      // Display 'bold' (markers hidden); display offset 2 is inside the
      // run's content.
      controller.applyProjectedSelection(const FlarkSelection.collapsed(2));
      await tester.pump();

      await _composeAndCommit(
        tester,
        editable,
        regionStart: 2,
        stages: const ['k', 'か', 'かん', '感'],
        commit: '感',
      );
      await _expectCommitted(
        tester,
        controller,
        display: 'bo感ld',
        source: '**bo感ld**',
      );
    });

    testWidgets('Korean jamo composition rewrites the cluster per keystroke', (
      tester,
    ) async {
      final controller = await _pumpLiveEditor(tester, '');
      final editable = find.byType(EditableText);
      await tester.showKeyboard(editable);
      await tester.pump();

      await _composeAndCommit(
        tester,
        editable,
        regionStart: 0,
        stages: const ['ㅎ', '하', '한'],
        commit: '한',
      );
      await _expectCommitted(tester, controller, display: '한', source: '한');

      // The next cluster opens a fresh composing region after the commit.
      await tester.showKeyboard(editable);
      await _composeAndCommit(
        tester,
        editable,
        regionStart: 1,
        stages: const ['ㄱ', '그', '글'],
        commit: '글',
      );
      await _expectCommitted(tester, controller, display: '한글', source: '한글');
    });

    testWidgets(
      'composition at a strong run trailing edge commits the space outside '
      'and stays armed',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, '**hello**');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        // Display 'hello'; the trailing display edge maps inside the run
        // (before the hidden closing marker).
        controller.applyProjectedSelection(const FlarkSelection.collapsed(5));
        await tester.pump();

        await _composeAndCommit(
          tester,
          editable,
          regionStart: 5,
          stages: const ['w', 'wo'],
          commit: 'wo ',
        );
        // The armed-continuation path under IME: the trailing space lands
        // outside the delimiter, styles stay armed.
        await _expectCommitted(
          tester,
          controller,
          display: 'hellowo ',
          source: '**hellowo** ',
        );

        // The next styled keystroke re-enters the run.
        await tester.showKeyboard(editable);
        await _sendIme(
          tester,
          const TextEditingValue(
            text: 'hellowo x',
            selection: TextSelection.collapsed(offset: 9),
          ),
        );
        await _expectCommitted(
          tester,
          controller,
          display: 'hellowo x',
          source: '**hellowo x**',
        );
      },
    );

    testWidgets(
      're-delivered identical platform value after an applied edit is '
      'swallowed',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, 'hello');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        controller.applyProjectedSelection(const FlarkSelection.collapsed(5));
        await tester.pump();

        const value = TextEditingValue(
          text: 'hello!',
          selection: TextSelection.collapsed(offset: 6),
        );
        await _sendIme(tester, value);
        expect(controller.markdown, 'hello!');

        // The platform delivers its own version of the already-applied
        // change (the echo); it must not apply twice.
        await _sendIme(tester, value);
        await _sendIme(tester, value);
        expect(controller.markdown, 'hello!');
        await _expectCommitted(
          tester,
          controller,
          display: 'hello!',
          source: 'hello!',
        );
      },
    );

    testWidgets(
      'identical echo after a marker-hiding edit does not double apply',
      (tester) async {
        // Same echo shape, but the applied edit lands inside a styled run,
        // so the editable's canonical text ('bold!') differs from the source
        // ('**bold!**') when the echo arrives.
        final controller = await _pumpLiveEditor(tester, '**bold**');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        controller.applyProjectedSelection(const FlarkSelection.collapsed(4));
        await tester.pump();

        const value = TextEditingValue(
          text: 'bold!',
          selection: TextSelection.collapsed(offset: 5),
        );
        await _sendIme(tester, value);
        expect(controller.markdown, '**bold!**');

        await _sendIme(tester, value);
        expect(controller.markdown, '**bold!**');
        await _expectCommitted(
          tester,
          controller,
          display: 'bold!',
          source: '**bold!**',
        );
      },
    );

    testWidgets(
      'backspace mid-composition shrinks the composing region before commit',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, '');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();

        // 'h' -> 'he' -> 'hel', then backspace shrinks the region to 'he'
        // (text and composing both shrink), then commit.
        await _composeAndCommit(
          tester,
          editable,
          regionStart: 0,
          stages: const ['h', 'he', 'hel', 'he'],
          commit: 'he',
        );
        await _expectCommitted(tester, controller, display: 'he', source: 'he');
      },
    );

    testWidgets(
      'composition inside an inline code span keeps whitespace inside and '
      'backticks untouched',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, 'a `code` b');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();
        // Display 'a code b'; display offset 6 is the span's trailing edge
        // and maps inside, before the hidden closing backtick.
        controller.applyProjectedSelection(const FlarkSelection.collapsed(6));
        await tester.pump();

        await _composeAndCommit(
          tester,
          editable,
          regionStart: 6,
          stages: const ['x'],
          commit: 'x ',
        );
        // Code spans are exempt from whitespace relocation: the trailing
        // space is code content and the backticks must not move.
        await _expectCommitted(
          tester,
          controller,
          display: 'a codex  b',
          source: 'a `codex ` b',
        );
      },
    );

    testWidgets(
      'composing over a widget-made selection replaces exactly the selected '
      'word',
      (tester) async {
        final controller = await _pumpLiveEditor(tester, 'hello world');
        final editable = find.byType(EditableText);
        await tester.showKeyboard(editable);
        await tester.pump();

        // The selection arrives from the platform like any IME/gesture
        // selection change: same text, new selection.
        await _sendIme(
          tester,
          const TextEditingValue(
            text: 'hello world',
            selection: TextSelection(baseOffset: 6, extentOffset: 11),
          ),
        );
        expect(
          controller.selection,
          const FlarkSelection(baseOffset: 6, extentOffset: 11),
        );

        // The first composing stage replaces the selected word wholesale.
        await _composeAndCommit(
          tester,
          editable,
          regionStart: 6,
          replaceLength: 5,
          stages: const ['wo', 'wonder'],
          commit: 'wonder ',
        );
        await _expectCommitted(
          tester,
          controller,
          display: 'hello wonder ',
          source: 'hello wonder ',
        );
      },
    );

    testWidgets('a stale-caret insertion (text updated, caret left behind) is '
        'normalized', (tester) async {
      // Some platforms deliver a pure insertion with the selection still at
      // the insertion point; the pipeline repairs the caret to follow the
      // inserted text (recognizer #2 in the intent-pipeline matrix).
      final controller = await _pumpLiveEditor(tester, '');
      final editable = find.byType(EditableText);
      await tester.showKeyboard(editable);
      await tester.pump();

      await _sendIme(
        tester,
        const TextEditingValue(
          text: 'a',
          selection: TextSelection.collapsed(offset: 0),
        ),
      );
      await _expectCommitted(tester, controller, display: 'a', source: 'a');
      expect(controller.selection, const FlarkSelection.collapsed(1));
    });
  });

  group('block surface (per-block live editables)', () {
    testWidgets('predictive composition inside a blockquote block widget', (
      tester,
    ) async {
      final controller = await _pumpLiveEditor(tester, '> quote');
      expect(find.byKey(const Key('FlarkLiveBlockBlockquote')), findsOneWidget);
      final editable = find.descendant(
        of: find.byKey(const Key('FlarkLiveBlockBlockquote')),
        matching: find.byType(EditableText),
      );
      controller.applySelection(
        const FlarkSelection.collapsed(7),
        userEvent: 'test',
      );
      await tester.pump();
      await tester.showKeyboard(editable);
      await tester.pump();

      // The block editable holds the block's display slice, not the whole
      // document — composing offsets below are block-local.
      expect(_remoteValue(tester, editable).text, 'quote');
      await _sendIme(
        tester,
        const TextEditingValue(
          text: 'quote ',
          selection: TextSelection.collapsed(offset: 6),
        ),
      );
      await _composeAndCommit(
        tester,
        editable,
        regionStart: 6,
        stages: const ['w', 'wo', 'wow'],
        commit: 'wow ',
      );
      await _expectCommitted(
        tester,
        controller,
        display: 'quote wow ',
        source: '> quote wow ',
      );
    });

    testWidgets('Japanese conversion inside a list item block widget', (
      tester,
    ) async {
      final controller = await _pumpLiveEditor(tester, '- item');
      expect(find.byKey(const Key('FlarkLiveBlockListMarker')), findsOneWidget);
      final editable = find.byType(EditableText);
      expect(editable, findsOneWidget);
      controller.applySelection(
        const FlarkSelection.collapsed(6),
        userEvent: 'test',
      );
      await tester.pump();
      await tester.showKeyboard(editable);
      await tester.pump();
      expect(_remoteValue(tester, editable).text, 'item');

      await _composeAndCommit(
        tester,
        editable,
        regionStart: 4,
        stages: const ['k', 'か', 'かん', '感'],
        commit: '感',
      );
      await _expectCommitted(
        tester,
        controller,
        display: 'item感',
        source: '- item感',
      );
    });
  });
}

/// Pumps [FlarkLiveRenderedEditableText] — the surface [FlarkMarkdownEditor]
/// mounts in live mode — around a fresh controller for [markdown], with an
/// authoritative comrak parse adopted before the first frame.
Future<FlarkFlutterController> _pumpLiveEditor(
  WidgetTester tester,
  String markdown,
) async {
  final controller = FlarkFlutterController.fromMarkdown(markdown);
  addTearDown(controller.dispose);
  await controller.parseNow();
  await tester.pumpWidget(
    Directionality(
      textDirection: TextDirection.ltr,
      child: FlarkLiveRenderedEditableText(
        controller: controller,
        style: const TextStyle(fontSize: 14),
      ),
    ),
  );
  await tester.pump();
  return controller;
}

/// The current value of the target editable — what the platform IME would
/// hold after the editor's last `setEditingState` echo.
TextEditingValue _remoteValue(WidgetTester tester, Finder editable) {
  return tester.widget<EditableText>(editable).controller.value;
}

/// Delivers one platform-shaped [TextEditingValue] over the engine test
/// text-input channel, exactly as an IME does, and pumps a frame.
Future<void> _sendIme(WidgetTester tester, TextEditingValue value) async {
  tester.testTextInput.updateEditingValue(value);
  await tester.pump();
}

/// Runs one IME composition session against [editable].
///
/// Models the platform shape shared by predictive keyboards and CJK
/// conversion: a composing region anchored at [regionStart] (editable-local
/// offsets) whose text is wholesale-replaced by each entry of [stages] — the
/// composing range covers the region and the collapsed caret sits at its end
/// — then [commit] replaces the region one final time with an empty composing
/// range. Stages may grow, shrink, or change length (conversion).
///
/// [replaceLength] simulates composing over a pre-selected range: the first
/// stage replaces that many code units after [regionStart].
///
/// The editable's current text is re-read before every update because the
/// editor may canonicalize the document (hide markers, relocate delimiters)
/// and echo the canonical text back to the IME — exactly what a real keyboard
/// sees via `setEditingState`.
///
/// After every update the helper asserts mid-composition fidelity: the
/// editable's text is exactly what the IME delivered (the editor never fights
/// the keyboard's view of the text) and — unless [expectComposingPreserved]
/// is false — the composing region survives the editor's rewrite (a dropped
/// or moved region desyncs real IMEs mid-flow). Pass
/// `expectComposingPreserved: false` only for a pinned defect, with a comment
/// pointing at the strict test that owns the contract.
Future<void> _composeAndCommit(
  WidgetTester tester,
  Finder editable, {
  required int regionStart,
  required List<String> stages,
  required String commit,
  int replaceLength = 0,
  bool expectComposingPreserved = true,
}) async {
  var regionLength = replaceLength;
  for (final stage in stages) {
    final text = _remoteValue(
      tester,
      editable,
    ).text.replaceRange(regionStart, regionStart + regionLength, stage);
    final composing = TextRange(
      start: regionStart,
      end: regionStart + stage.length,
    );
    await _sendIme(
      tester,
      TextEditingValue(
        text: text,
        selection: TextSelection.collapsed(offset: regionStart + stage.length),
        composing: composing,
      ),
    );
    final echoed = _remoteValue(tester, editable);
    expect(
      echoed.text,
      text,
      reason:
          'mid-composition display fidelity: the editor rewrote the text the '
          'IME delivered for stage "$stage"',
    );
    if (expectComposingPreserved) {
      expect(
        echoed.composing,
        composing,
        reason:
            'the editor dropped or moved the composing region for stage '
            '"$stage" — real IMEs desync when composition is not preserved',
      );
    }
    regionLength = stage.length;
  }
  final text = _remoteValue(
    tester,
    editable,
  ).text.replaceRange(regionStart, regionStart + regionLength, commit);
  await _sendIme(
    tester,
    TextEditingValue(
      text: text,
      selection: TextSelection.collapsed(offset: regionStart + commit.length),
    ),
  );
  final committed = _remoteValue(tester, editable);
  expect(
    committed.text,
    text,
    reason:
        'commit fidelity: the editor rewrote the committed text '
        '"$commit" the IME delivered',
  );
  expect(
    committed.composing,
    TextRange.empty,
    reason: 'the composing region must collapse on commit',
  );
}

/// Asserts the two commit-level invariants from
/// docs/architecture/v2/inline_delimiter_validity_2026-07-10.md after
/// settling the authoritative parse:
///
/// 1. **Display fidelity** — the projected display equals [display], exactly
///    what the user semantically typed.
/// 2. **Export round-trip** — a fresh, caret-free controller parsing the
///    exported `controller.markdown` projects the identical display, so the
///    source never depends on editor-local state (armed styles, caret
///    affinity, composition).
///
/// [source] additionally pins the canonical markdown where the scenario
/// dictates one.
Future<void> _expectCommitted(
  WidgetTester tester,
  FlarkFlutterController controller, {
  required String display,
  String? source,
}) async {
  await controller.parseNow();
  await tester.pump();
  if (source != null) {
    expect(controller.markdown, source, reason: 'canonical source');
  }
  expect(
    controller.projection.projectText(controller.markdown),
    display,
    reason:
        'display fidelity: source "${controller.markdown}" must project '
        'exactly what the user typed',
  );
  final export = controller.markdown;
  final fresh = FlarkFlutterController.fromMarkdown(export);
  try {
    expect(
      fresh.tryParseSync(),
      isTrue,
      reason: 'export round-trip needs the sync-capable comrak backend',
    );
    expect(
      fresh.projection.projectText(export),
      display,
      reason:
          'export round-trip: "$export" renders differently with no caret '
          'context — the source depends on editor-local state',
    );
  } finally {
    fresh.dispose();
  }
}
