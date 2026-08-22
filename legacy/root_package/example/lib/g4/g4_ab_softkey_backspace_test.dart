// RFC 024 Gate G4 — the one differential the bake-off turns on, run against
// BOTH variants in one file so it cannot be argued from reasoning alone.
//
// Variant A's own report named this as the functional hole it could not close:
// "soft-keyboard backspace at block offset 0. Android IMEs send a whole new
// value; deleting at offset 0 of a block produces *no change* in the block-local
// text, so the controller diff sees nothing and the block merge silently doesn't
// happen." Hardware backspace works in both (acceptance case 14, via
// `DeleteCharacterIntent`) — this is specifically the soft-keyboard path, which
// is the ONLY path on phones.
//
// Run:  flutter test lib/g4/g4_ab_softkey_backspace_test.dart

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_test/flutter_test.dart';

import 'g4_model.dart';
import 'g4_surface.dart';
import 'g4_variant_a.dart';
import 'g4_variant_b.dart';

TextEditingValue platformValue(WidgetTester tester) {
  final Map<String, dynamic> st = tester.testTextInput.editingState!;
  return TextEditingValue(
    text: st['text'] as String,
    selection: TextSelection(
      baseOffset: st['selectionBase'] as int,
      extentOffset: st['selectionExtent'] as int,
    ),
  );
}

Future<
  ({G4Document doc, G4SurfaceState<G4Surface> surface})
>
pump(WidgetTester tester, G4Surface Function({
  required Key key,
  required G4Document document,
  required ScrollController scrollController,
}) build) async {
  final G4Document doc = G4Document(g4FixtureBlocks(count: 20));
  final ScrollController scroll = ScrollController();
  final GlobalKey<G4SurfaceState<G4Surface>> key = GlobalKey<G4SurfaceState<G4Surface>>();
  addTearDown(scroll.dispose);
  await tester.pumpWidget(
    MaterialApp(
      home: Scaffold(
        body: Center(child: build(key: key, document: doc, scrollController: scroll)),
      ),
    ),
  );
  await tester.pump();
  key.currentState!.setSelection(const G4Selection.collapsed(G4Position(3, 0)));
  await tester.pump();
  await tester.pump();
  return (doc: doc, surface: key.currentState!);
}

void main() {
  testWidgets('A(editable-island): the platform caret sits at buffer offset 0, so the '
      'backspace is UNREPORTABLE and the merge never happens', (WidgetTester tester) async {
    final ({G4Document doc, G4SurfaceState<G4Surface> surface}) h =
        await pump(tester, G4VariantA.builder);

    final TextEditingValue v = platformValue(tester);
    expect(
      v.text,
      'Block 003 alpha bravo charlie',
      reason: 'the island hands the platform exactly one block, verbatim',
    );
    expect(
      v.selection.baseOffset,
      0,
      reason: 'if this is ever > 0 the island has grown an invisible prefix and the hole is closed',
    );

    // There is nothing before the caret for the IME to delete. The best a soft
    // keyboard can do is re-send the value it already has.
    tester.testTextInput.updateEditingValue(v);
    await tester.pump();

    expect(
      h.doc.blockCount,
      20,
      reason: 'if this ever becomes 19 the hole is closed and this test should be deleted',
    );
    expect(h.doc.blockAt(2), 'Block 002 alpha bravo charlie');
    expect(h.doc.blockAt(3), 'Block 003 alpha bravo charlie');
  });

  testWidgets('B(own-painted): the invisible prefix makes the backspace reportable, '
      'and it merges the blocks', (WidgetTester tester) async {
    final ({G4Document doc, G4SurfaceState<G4Surface> surface}) h =
        await pump(tester, G4VariantB.builder);

    final TextEditingValue v = platformValue(tester);
    expect(v.text.startsWith(kImePrefix), isTrue);
    expect(v.selection.baseOffset, kImePrefix.length);

    final int caret = v.selection.baseOffset;
    tester.testTextInput.updateEditingValue(
      TextEditingValue(
        text: v.text.substring(0, caret - 1) + v.text.substring(caret),
        selection: TextSelection.collapsed(offset: caret - 1),
      ),
    );
    await tester.pump();

    expect(h.doc.blockCount, 19);
    expect(h.doc.blockAt(2), 'Block 002 alpha bravo charlieBlock 003 alpha bravo charlie');
    expect(h.surface.selection!.extent, const G4Position(2, 29));
  });
}
