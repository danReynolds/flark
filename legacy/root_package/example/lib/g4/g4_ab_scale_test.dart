// RFC 024 Gate G4 — the §7 scale criterion neither variant had run:
// "`Cmd+A` then copy on a 1 MB document yielding complete exact source".
//
// The shared acceptance suite runs at 400 blocks (~11 KB). This runs the same
// select-all + copy at ~1 MB against BOTH variants, and also checks that
// virtualization still holds at that size. Correctness only — this is not a
// frame-timing harness and makes no performance claim (G2 owns that).
//
// Run:  flutter test lib/g4/g4_ab_scale_test.dart

import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';

import 'g4_model.dart';
import 'g4_surface.dart';
import 'g4_variant_a.dart';
import 'g4_variant_b.dart';

/// ~1.05 MB of source.
const int kBigBlockCount = 34000;

const List<G4Variant> _variants = <G4Variant>[
  G4Variant('A(editable-island)', G4VariantA.builder),
  G4Variant('B(own-painted)', G4VariantB.builder),
];

void main() {
  for (final G4Variant variant in _variants) {
    testWidgets('${variant.name} select-all + copy on a ~1 MB document', (
      WidgetTester tester,
    ) async {
      final G4Document doc = G4Document(g4FixtureBlocks(count: kBigBlockCount));
      final ScrollController scroll = ScrollController();
      final GlobalKey<G4SurfaceState<G4Surface>> key =
          GlobalKey<G4SurfaceState<G4Surface>>();
      addTearDown(scroll.dispose);

      final String source = doc.text;
      expect(source.length, greaterThan(1000000), reason: 'fixture is not 1 MB');

      await tester.pumpWidget(
        MaterialApp(
          home: Scaffold(
            body: Center(
              child: variant.build(key: key, document: doc, scrollController: scroll),
            ),
          ),
        ),
      );
      await tester.pump();

      // Virtualization must still hold at 34k blocks.
      expect(find.byKey(g4BlockKey(0)).evaluate(), isNotEmpty);
      expect(find.byKey(g4BlockKey(17000)).evaluate(), isEmpty);
      expect(find.byKey(g4BlockKey(kBigBlockCount - 1)).evaluate(), isEmpty);

      key.currentState!.setSelection(doc.selectAll);
      await tester.pump();
      await tester.pump();

      final Stopwatch sw = Stopwatch()..start();
      final String copied = key.currentState!.copySelection();
      sw.stop();

      expect(copied.length, source.length);
      expect(copied, source);
      expect(copied.startsWith('Block 000 alpha bravo charlie'), isTrue);
      expect(copied.endsWith('Block 33999 alpha bravo charlie'), isTrue);
      expect(find.byKey(g4BlockKey(17000)).evaluate(), isEmpty,
          reason: 'select-all built the document');

      // Reported, not asserted: this is a widget test on a dev machine, not a
      // floor-device measurement.
      // ignore: avoid_print
      print(
        '    [${variant.name}] extractRange over ${source.length} chars / '
        '$kBigBlockCount blocks: ${sw.elapsedMicroseconds / 1000} ms',
      );
    });
  }
}
