// Shared render-model decode + projection (identical to the FFI harness).
import 'dart:convert';
import 'dart:typed_data';

const headerWords = 9, blockWords = 12, contentWords = 5, runWords = 13;
const kText = 1, kEmph = 2, kStrong = 3, kCode = 4, kStrike = 5, kLink = 6, kAutolink = 8, kEscape = 9, kReplacement = 10, kHard = 11, kSoft = 12;
const bParagraph = 1, bHeading = 2, bCell = 11;

String gen(int target) {
  final s = StringBuffer();
  var i = 0;
  while (s.length < target) {
    i++;
    s.write('## Section $i\n\nThis is a paragraph with *emphasis*, **strong**, `code`, a [link](https://example.com/$i) and some ~~struck~~ text that wraps across several lines of ordinary prose so that the row is realistic.\n\n- item one with *em*\n- item two with **strong**\n  - nested item\n\n> a quote with `code` inside\n\n```dart\nvoid main() { print(\'hi $i\'); }\n```\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n[ref$i]: https://example.com/ref$i\n\n');
  }
  return s.toString();
}

class Row { Row(this.block, this.text, this.segments); final int block; final String text; final Int32List segments; }

class Model {
  Model(this.bytes) : data = ByteData.sublistView(bytes) {
    lineCount = data.getUint32(16, Endian.little); blockCount = data.getUint32(20, Endian.little);
    runCount = data.getUint32(24, Endian.little); contentCount = data.getUint32(28, Endian.little);
    blocksOff = (headerWords + lineCount * 2) * 4; contentOff = blocksOff + blockCount * blockWords * 4;
    runsOff = contentOff + contentCount * contentWords * 4; stringsOff = runsOff + runCount * runWords * 4;
  }
  final Uint8List bytes; final ByteData data;
  late final int blockCount, runCount, contentCount, lineCount, blocksOff, contentOff, runsOff, stringsOff;
  int block(int i, int w) => data.getUint32(blocksOff + (i * blockWords + w) * 4, Endian.little);
  int run(int i, int w) => data.getUint32(runsOff + (i * runWords + w) * 4, Endian.little);
  String str(int off, int len) => utf8.decode(Uint8List.sublistView(bytes, stringsOff + off, stringsOff + off + len));
}

List<Row> project(Model m, String src) {
  final rows = <Row>[]; final style = Int32List(m.runCount); var r = 0;
  for (var b = 0; b < m.blockCount; b++) {
    final kind = m.block(b, 0);
    final leaf = kind == bParagraph || kind == bHeading || kind == bCell;
    if (!leaf) { while (r < m.runCount && m.run(r, 1) == b) r++; continue; }
    final text = StringBuffer(); final segs = <int>[];
    while (r < m.runCount && m.run(r, 1) == b) {
      final k = m.run(r, 0), parent = m.run(r, 2);
      final inherited = parent == 0xFFFFFFFF ? 0 : style[parent];
      style[r] = inherited | switch (k) { kEmph => 1, kStrong => 2, kCode => 4, kStrike => 8, kLink => 16, _ => 0 };
      final cs = m.run(r, 9), ce = m.run(r, 10);
      switch (k) {
        case kText || kCode || kAutolink || kEscape: if (ce > cs) { final d0 = text.length; text.write(src.substring(cs, ce)); segs.addAll([cs, ce, d0, text.length, style[r]]); }
        case kReplacement: final d0 = text.length; text.write(m.str(m.run(r, 11), m.run(r, 12))); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        case kSoft: final d0 = text.length; text.write(' '); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        case kHard: final d0 = text.length; text.write('\n'); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        default: break;
      }
      r++;
    }
    rows.add(Row(b, text.toString(), Int32List.fromList(segs)));
  }
  return rows;
}
