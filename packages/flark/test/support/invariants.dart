/// Projection invariants shared by the corpus test and the journeys.
library;

import 'package:flark/flark.dart';
import 'package:test/test.dart';

void checkInvariants(String src, RenderModel m, Projection p, String label) {
  // Hidden intervals across the document: run delimiters and break markers.
  final hidden = <(int, int)>[];
  for (final r in m.runs) {
    if (r.kind == RunKind.softBreak || r.kind == RunKind.hardBreak) { if (r.endUtf16 > r.startUtf16) hidden.add((r.startUtf16, r.endUtf16)); continue; }
    if (r.contentStartUtf16 > r.startUtf16) hidden.add((r.startUtf16, r.contentStartUtf16));
    if (r.endUtf16 > r.contentEndUtf16) hidden.add((r.contentEndUtf16, r.endUtf16));
  }
  bool inHidden(int a, int b) => hidden.any((h) => h.$1 < b && h.$2 > a);
  final ownedLines = <int>{};
  for (final row in p.rows) {
    // Display text is the concatenation of its segments, in order and contiguous.
    var expectDisplay = 0;
    final rebuilt = StringBuffer();
    for (final s in row.segments) {
      expect(s.displayStart, expectDisplay, reason: '$label row ${row.index}: segments not contiguous');
      expectDisplay = s.displayEnd;
      final piece = row.text.substring(s.displayStart, s.displayEnd);
      rebuilt.write(piece);
      if (s.exact) {
        expect(piece, src.substring(s.sourceStart, s.sourceEnd), reason: '$label row ${row.index}: exact segment differs from source');
        expect(inHidden(s.sourceStart, s.sourceEnd), isFalse, reason: '$label row ${row.index}: display shows hidden bytes ${s.sourceStart}..${s.sourceEnd}');
      }
      expect(s.sourceStart <= s.sourceEnd, isTrue);
    }
    expect(rebuilt.toString(), row.text, reason: '$label row ${row.index}');
    expect(expectDisplay, row.text.length, reason: '$label row ${row.index}: segments do not cover the text');
    // Round trip through every display offset on exact segments.
    for (final s in row.segments.where((s) => s.exact)) {
      for (var d = s.displayStart; d <= s.displayEnd; d++) {
        final source = row.sourceForDisplay(d);
        final (back, snapped) = row.displayForSource(source);
        expect(back, d, reason: '$label row ${row.index}: display $d -> source $source -> display $back');
        expect(snapped, isFalse);
      }
    }
    for (var l = row.firstLine; l < row.firstLine + row.lineCount; l++) { ownedLines.add(l); }
  }
  // Every line is owned by a row or hidden inside a leaf block or table.
  for (var l = 0; l < m.lineCount; l++) {
    if (ownedLines.contains(l)) continue;
    final coveredByLeaf = m.blocks.any((b) { final k = b.kind; final isLeaf = k == BlockKind.paragraph || k == BlockKind.heading || k == BlockKind.codeBlock || k == BlockKind.htmlBlock || k == BlockKind.table; return isLeaf && l >= b.firstLine && l < b.firstLine + b.lineCount; });
    expect(coveredByLeaf, isTrue, reason: '$label: line $l is neither a row nor hidden');
  }
}

