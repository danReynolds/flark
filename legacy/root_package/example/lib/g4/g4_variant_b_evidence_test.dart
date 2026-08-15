// RFC 024 Gate G4 — Variant-B-only evidence.
//
// The shared acceptance suite (g4_acceptance_test.dart) is deliberately not
// touched by this file: it must keep running identically for both variants.
// This is the mirror of the `A(editable-island) EVIDENCE` group — the proof
// behind Variant B's claims in the report, including the two holes Variant A
// could not close.
//
// Run:  flutter test lib/g4/g4_variant_b_evidence_test.dart

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'g4_model.dart';
import 'g4_surface.dart';
import 'g4_variant_b.dart';

const int kBlockCount = 400;

class _H {
  _H(this.document, this.scroll, this.key);
  final G4Document document;
  final ScrollController scroll;
  final GlobalKey<G4VariantBState> key;
  G4VariantBState get surface => key.currentState!;
}

Future<_H> pumpB(WidgetTester tester, {int count = kBlockCount}) async {
  final G4Document doc = G4Document(g4FixtureBlocks(count: count));
  final ScrollController scroll = ScrollController();
  final GlobalKey<G4VariantBState> key = GlobalKey<G4VariantBState>();
  addTearDown(scroll.dispose);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(
          child: G4VariantB(key: key, document: doc, scrollController: scroll),
        ),
      ),
    ),
  );
  await tester.pump();
  return _H(doc, scroll, key);
}

/// What the platform IME currently believes it is holding.
TextEditingValue platformValue(WidgetTester tester) {
  final Map<String, dynamic> st = tester.testTextInput.editingState!;
  return TextEditingValue(
    text: st['text'] as String,
    selection: TextSelection(
      baseOffset: st['selectionBase'] as int,
      extentOffset: st['selectionExtent'] as int,
    ),
    composing: TextRange(
      start: st['composingBase'] as int,
      end: st['composingExtent'] as int,
    ),
  );
}

/// Non-delta ingress (what `TestTextInput` and older engines send).
Future<void> platformSend(WidgetTester tester, TextEditingValue v) async {
  tester.testTextInput.updateEditingValue(v);
  await tester.pump();
}

/// TRUE delta ingress — the message a real platform sends when
/// `TextInputConfiguration.enableDeltaModel` is set. Client id -1 is the
/// documented debug-mode wildcard (text_input.dart:2244).
Future<void> platformSendDelta(
  WidgetTester tester, {
  required String oldText,
  required int deltaStart,
  required int deltaEnd,
  required String deltaText,
  required int selectionBase,
  required int selectionExtent,
  int composingBase = -1,
  int composingExtent = -1,
}) async {
  await tester.binding.defaultBinaryMessenger.handlePlatformMessage(
    SystemChannels.textInput.name,
    SystemChannels.textInput.codec.encodeMethodCall(
      MethodCall('TextInputClient.updateEditingStateWithDeltas', <dynamic>[
        -1,
        <String, dynamic>{
          'deltas': <Map<String, dynamic>>[
            <String, dynamic>{
              'oldText': oldText,
              'deltaStart': deltaStart,
              'deltaEnd': deltaEnd,
              'deltaText': deltaText,
              'selectionBase': selectionBase,
              'selectionExtent': selectionExtent,
              'composingBase': composingBase,
              'composingExtent': composingExtent,
            },
          ],
        },
      ]),
    ),
    (_) {},
  );
  await tester.pump();
}

bool blockIsBuilt(int index) => find.byKey(g4BlockKey(index)).evaluate().isNotEmpty;

void main() {
  group('B(own-painted) EVIDENCE', () {
    testWidgets('E1. soft-keyboard backspace at block offset 0 MERGES blocks '
        '(the hole Variant A documented and could not close)', (WidgetTester tester) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 0)));
      await tester.pump();
      await tester.pump();

      final TextEditingValue v = platformValue(tester);
      expect(v.selection.isCollapsed, isTrue);
      // The whole point of the invisible prefix: at model offset 0 of a block,
      // the platform caret is NOT at buffer offset 0, so the soft keyboard has
      // something to delete and will actually report the backspace.
      expect(
        v.selection.baseOffset,
        greaterThan(0),
        reason: 'no invisible prefix — an Android IME would report nothing here',
      );
      expect(v.text.startsWith(kImePrefix), isTrue);

      // A soft keyboard does not send a key event; it sends a whole new value
      // with one character removed. This is exactly the traffic that produced
      // "no change" in Variant A and silently dropped the merge.
      final int caret = v.selection.baseOffset;
      await platformSend(
        tester,
        TextEditingValue(
          text: v.text.substring(0, caret - 1) + v.text.substring(caret),
          selection: TextSelection.collapsed(offset: caret - 1),
        ),
      );

      expect(h.document.blockCount, kBlockCount - 1);
      expect(
        h.document.blockAt(2),
        'Block 002 alpha bravo charlieBlock 003 alpha bravo charlie',
      );
      expect(h.surface.selection!.extent, const G4Position(2, 29));
      expect(h.surface.selection!.isCollapsed, isTrue);
    });

    testWidgets('E2. the REAL delta path drives the model, not just the synthesised one', (
      WidgetTester tester,
    ) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(const G4Selection.collapsed(G4Position(4, 6)));
      await tester.pump();
      await tester.pump();

      final TextEditingValue v = platformValue(tester);
      // Insert "zz" at model (4,6) == buffer offset prefix+6.
      await platformSendDelta(
        tester,
        oldText: v.text,
        deltaStart: v.selection.baseOffset,
        deltaEnd: v.selection.baseOffset,
        deltaText: 'zz',
        selectionBase: v.selection.baseOffset + 2,
        selectionExtent: v.selection.baseOffset + 2,
      );

      expect(h.document.blockAt(4), 'Block zz004 alpha bravo charlie');
      expect(h.document.blockCount, kBlockCount);
      expect(h.surface.selection!.extent, const G4Position(4, 8));
    });

    testWidgets('E3. the connection really is registered as a delta client', (
      WidgetTester tester,
    ) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(const G4Selection.collapsed(G4Position(1, 1)));
      await tester.pump();
      await tester.pump();

      expect(tester.testTextInput.hasAnyClients, isTrue);
      expect(tester.testTextInput.setClientArgs!['enableDeltaModel'], isTrue);
      expect(tester.testTextInput.setClientArgs!['autocorrect'], isTrue);
      expect(tester.testTextInput.setClientArgs!['enableSuggestions'], isTrue);
    });

    testWidgets('E4. select-all does NOT push the document into the IME buffer', (
      WidgetTester tester,
    ) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(h.document.selectAll);
      await tester.pump();
      await tester.pump();

      expect(h.document.text.length, greaterThan(10000));
      final TextEditingValue v = platformValue(tester);
      // Bound written as a literal on purpose: asserting against
      // `kImeWindowMaxChars` would still pass if someone raised the cap.
      expect(
        v.text.length,
        lessThanOrEqualTo(4096),
        reason: 'the window cap did not engage — 1 MB would go to the platform IME',
      );
      // ...and the model still holds the whole selection.
      expect(h.surface.copySelection(), h.document.text);
    });

    testWidgets('E5. while the window is clipped, an IME replacement is widened '
        'back out to the full model selection', (WidgetTester tester) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(h.document.selectAll);
      await tester.pump();
      await tester.pump();

      final TextEditingValue v = platformValue(tester);
      expect(
        v.selection.isCollapsed,
        isFalse,
        reason: 'the clipped window must still show the caret block\'s share',
      );

      await platformSend(
        tester,
        TextEditingValue(
          text: '${v.text.substring(0, v.selection.start)}X'
              '${v.text.substring(v.selection.end)}',
          selection: TextSelection.collapsed(offset: v.selection.start + 1),
        ),
      );

      expect(
        h.document.blockCount,
        1,
        reason: 'the replacement was applied only to the visible window',
      );
      expect(h.document.blockAt(0), 'X');
    });

    testWidgets('E6. there is no EditableText anywhere, and the IME connection does not '
        'depend on any block being built', (WidgetTester tester) async {
      final _H h = await pumpB(tester);
      expect(find.byType(EditableText), findsNothing);

      h.surface.setSelection(const G4Selection.collapsed(G4Position(2, 5)));
      await tester.pump();
      await tester.pump();
      expect(find.byType(EditableText), findsNothing);
      expect(tester.testTextInput.hasAnyClients, isTrue);

      // Scroll the caret block far out of the tree. No keep-alive, no overlay,
      // no hand-positioned island: nothing here can be recycled away.
      h.scroll.jumpTo(200 * G4Layout.itemExtent);
      await tester.pump();
      expect(blockIsBuilt(2), isFalse);
      expect(tester.testTextInput.hasAnyClients, isTrue);

      final TextEditingValue v = platformValue(tester);
      await platformSend(
        tester,
        TextEditingValue(
          text: '${v.text.substring(0, v.selection.baseOffset)}Q'
              '${v.text.substring(v.selection.baseOffset)}',
          selection: TextSelection.collapsed(offset: v.selection.baseOffset + 1),
        ),
      );
      expect(h.document.blockAt(2), 'BlockQ 002 alpha bravo charlie');
      expect(blockIsBuilt(2), isFalse, reason: 'editing must not have forced a build');
    });

    testWidgets('E7. macOS performSelector routes to the document', (WidgetTester tester) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 0)));
      await tester.pump();
      await tester.pump();

      // What Flutter delivers for a macOS `deleteBackward:` selector.
      h.surface.performSelector('deleteBackward:');
      await tester.pump();

      expect(h.document.blockCount, kBlockCount - 1);
      expect(
        h.document.blockAt(2),
        'Block 002 alpha bravo charlieBlock 003 alpha bravo charlie',
      );
    });

    testWidgets('E9. a soft-keyboard Enter (a lone \\n) splits the block', (
      WidgetTester tester,
    ) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(const G4Selection.collapsed(G4Position(3, 9)));
      await tester.pump();
      await tester.pump();

      final TextEditingValue v = platformValue(tester);
      final int caret = v.selection.baseOffset;
      await platformSend(
        tester,
        TextEditingValue(
          text: '${v.text.substring(0, caret)}\n${v.text.substring(caret)}',
          selection: TextSelection.collapsed(offset: caret + 1),
        ),
      );

      expect(h.document.blockCount, kBlockCount + 1);
      expect(h.document.blockAt(3), 'Block 003');
      expect(h.document.blockAt(4), ' alpha bravo charlie');
      expect(h.surface.selection!.extent, const G4Position(4, 0));
      // The buffer and the model diverged by one character, so the surface must
      // have resynced rather than trusting the platform's post-edit offsets.
      expect(platformValue(tester).text, '$kImePrefix alpha bravo charlie');
    });

    testWidgets('E8. the IME window is the model\'s own serialisation — window offsets '
        'and model coordinates are the same edit', (WidgetTester tester) async {
      final _H h = await pumpB(tester);
      h.surface.setSelection(
        const G4Selection(base: G4Position(2, 3), extent: G4Position(4, 7)),
      );
      await tester.pump();
      await tester.pump();

      final TextEditingValue v = platformValue(tester);
      expect(
        v.text,
        '$kImePrefix${h.document.blockAt(2)}\n\n${h.document.blockAt(3)}\n\n'
        '${h.document.blockAt(4)}',
      );
      expect(
        v.selection.textInside(v.text),
        h.document.extractRange(h.surface.selection!),
        reason: 'the platform is not looking at the same characters the model is',
      );
    });
  });
}
