// End-to-end keystroke spike: splice -> encode -> FFI parse -> decode -> project.
import 'dart:convert';
import 'dart:ffi';
import 'dart:io';
import 'dart:math';
import 'dart:typed_data';

typedef ParseC = Int32 Function(Pointer<Uint8>, Uint32, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef ParseD = int Function(Pointer<Uint8>, int, Pointer<Pointer<Uint8>>, Pointer<Uint32>);
typedef AllocC = Pointer<Uint8> Function(Uint32);
typedef AllocD = Pointer<Uint8> Function(int);
typedef FreeC = Void Function(Pointer<Uint8>, Uint32);
typedef FreeD = void Function(Pointer<Uint8>, int);

const headerWords = 9, blockWords = 12, contentWords = 5, runWords = 13;
const kText = 1, kEmph = 2, kStrong = 3, kCode = 4, kStrike = 5, kLink = 6, kImage = 7, kAutolink = 8, kEscape = 9, kReplacement = 10, kHard = 11, kSoft = 12;
const bParagraph = 1, bHeading = 2, bCodeBlock = 3, bCell = 11;

String gen(int target) {
  final s = StringBuffer();
  var i = 0;
  while (s.length < target) {
    i++;
    s.write('## Section $i\n\nThis is a paragraph with *emphasis*, **strong**, `code`, a [link](https://example.com/$i) and some ~~struck~~ text that wraps across several lines of ordinary prose so that the row is realistic.\n\n- item one with *em*\n- item two with **strong**\n  - nested item\n\n> a quote with `code` inside\n\n```dart\nvoid main() { print(\'hi $i\'); }\n```\n\n| a | b |\n|---|---|\n| 1 | *2* |\n\n[ref$i]: https://example.com/ref$i\n\n');
  }
  return s.toString();
}

/// One projected row: display text plus segments mapping display ranges to source UTF-16 ranges.
class Row {
  Row(this.block, this.text, this.segments);
  final int block; final String text; final Int32List segments; // [srcStart, srcEnd, dispStart, dispEnd, style] * n
}

class Model {
  Model(this.bytes) : data = ByteData.sublistView(bytes) {
    blockCount = data.getUint32(5 * 4, Endian.little);
    runCount = data.getUint32(6 * 4, Endian.little);
    contentCount = data.getUint32(7 * 4, Endian.little);
    stringBytes = data.getUint32(8 * 4, Endian.little);
    lineCount = data.getUint32(4 * 4, Endian.little);
    blocksOff = (headerWords + lineCount * 2) * 4;
    contentOff = blocksOff + blockCount * blockWords * 4;
    runsOff = contentOff + contentCount * contentWords * 4;
    stringsOff = runsOff + runCount * runWords * 4;
  }
  final Uint8List bytes; final ByteData data;
  late final int blockCount, runCount, contentCount, stringBytes, lineCount, blocksOff, contentOff, runsOff, stringsOff;
  int block(int i, int w) => data.getUint32(blocksOff + (i * blockWords + w) * 4, Endian.little);
  int run(int i, int w) => data.getUint32(runsOff + (i * runWords + w) * 4, Endian.little);
  String str(int off, int len) => utf8.decode(Uint8List.sublistView(bytes, stringsOff + off, stringsOff + off + len));
}

/// Project every leaf block into rows. Runs are contiguous per block in document order.
List<Row> project(Model m, String src, {Map<String, Row>? memo, Map<String, Row>? nextMemo}) {
  final rows = <Row>[];
  final style = Int32List(m.runCount);
  var r = 0;
  for (var b = 0; b < m.blockCount; b++) {
    final kind = m.block(b, 0);
    final leaf = kind == bParagraph || kind == bHeading || kind == bCell;
    // advance r to this block's first run
    if (!leaf) { while (r < m.runCount && m.run(r, 1) == b) r++; continue; }
    final s16 = m.block(b, 4), e16 = m.block(b, 5);
    String? key;
    if (memo != null) {
      key = '$kind:${src.substring(s16, e16)}';
      final hit = memo[key];
      if (hit != null) { rows.add(hit); nextMemo![key] = hit; while (r < m.runCount && m.run(r, 1) == b) r++; continue; }
    }
    final text = StringBuffer();
    final segs = <int>[];
    while (r < m.runCount && m.run(r, 1) == b) {
      final k = m.run(r, 0), parent = m.run(r, 2);
      final inherited = parent == 0xFFFFFFFF ? 0 : style[parent];
      final own = switch (k) { kEmph => 1, kStrong => 2, kCode => 4, kStrike => 8, kLink => 16, _ => 0 };
      style[r] = inherited | own;
      final cs = m.run(r, 9), ce = m.run(r, 10);
      switch (k) {
        case kText || kCode || kAutolink || kEscape:
          if (ce > cs) { final d0 = text.length; text.write(src.substring(cs, ce)); segs.addAll([cs, ce, d0, text.length, style[r]]); }
        case kReplacement:
          final d0 = text.length; text.write(m.str(m.run(r, 11), m.run(r, 12))); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        case kSoft: final d0 = text.length; text.write(' '); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        case kHard: final d0 = text.length; text.write('\n'); segs.addAll([m.run(r, 7), m.run(r, 8), d0, text.length, style[r]]);
        default: break; // containers: children carry the text
      }
      r++;
    }
    final row = Row(b, text.toString(), Int32List.fromList(segs));
    rows.add(row);
    if (key != null) nextMemo![key] = row;
  }
  return rows;
}

void main(List<String> args) {
  final libPath = args.isNotEmpty ? args[0] : '../parse_spike/target/release/libflark_parse_spike.dylib';
  final lib = DynamicLibrary.open(libPath);
  final parse = lib.lookupFunction<ParseC, ParseD>('flark_spike_parse');
  final alloc = lib.lookupFunction<AllocC, AllocD>('flark_spike_alloc');
  final free = lib.lookupFunction<FreeC, FreeD>('flark_spike_free');
  final outCell = alloc(16);
  final outPtr = outCell.cast<Pointer<Uint8>>();
  final outLen = Pointer<Uint32>.fromAddress(outCell.address + 8);
  final rng = Random(7);
  for (final size in [25000, 64000, 100000]) {
    var src = gen(size);
    final cap = size * 2;
    final input = alloc(cap);
    final inputView = input.asTypedList(cap);
    for (final memoOn in [false, true]) {
      Map<String, Row> memo = {};
      final tSplice = <double>[], tEncode = <double>[], tCopy = <double>[], tParse = <double>[], tProject = <double>[], tTotal = <double>[];
      var modelBytes = 0, rowsOut = 0;
      final n = 300;
      for (var k = 0; k < n; k++) {
        final pos = rng.nextInt(src.length);
        final ch = const ['x', ' ', '*', 'a'][rng.nextInt(4)];
        final sw = Stopwatch()..start();
        src = src.substring(0, pos) + ch + src.substring(pos);
        final a = sw.elapsedMicroseconds;
        final bytes = utf8.encode(src);
        final b = sw.elapsedMicroseconds;
        inputView.setRange(0, bytes.length, bytes);
        final c = sw.elapsedMicroseconds;
        final rc = parse(input, bytes.length, outPtr, outLen);
        if (rc != 0) throw StateError('parse rc=$rc');
        final d = sw.elapsedMicroseconds;
        final len = outLen.value;
        final view = outPtr.value.asTypedList(len);
        final model = Model(view);
        final next = <String, Row>{};
        final rows = project(model, src, memo: memoOn ? memo : null, nextMemo: memoOn ? next : null);
        final e = sw.elapsedMicroseconds;
        free(outPtr.value, len);
        if (memoOn) memo = next;
        modelBytes = len; rowsOut = rows.length;
        if (k >= 20) { tSplice.add((a) / 1000); tEncode.add((b - a) / 1000); tCopy.add((c - b) / 1000); tParse.add((d - c) / 1000); tProject.add((e - d) / 1000); tTotal.add(e / 1000); }
      }
      double p(List<double> l, double q) { final s = [...l]..sort(); return s[(s.length * q).floor().clamp(0, s.length - 1)]; }
      String f(List<double> l) => '${p(l, .5).toStringAsFixed(2)}/${p(l, .9).toStringAsFixed(2)}/${p(l, .99).toStringAsFixed(2)}';
      print('${src.length.toString().padLeft(7)} B memo=${memoOn ? 'on ' : 'off'}  total ${f(tTotal)}  | splice ${f(tSplice)}  encode ${f(tEncode)}  copy ${f(tCopy)}  parse+extract ${f(tParse)}  decode+project ${f(tProject)}  (ms p50/p90/p99; model ${(modelBytes / 1024).round()} KiB, $rowsOut rows)');
    }
    free(input, cap);
  }
}
