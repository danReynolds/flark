import 'dart:typed_data';

import 'package:test/test.dart';

import '../../tool/parser_research/dart/incremental_delta_codec.dart';

void main() {
  test('Dart syntax delta matches the Rust little-endian golden', () {
    final delta = PrototypeSyntaxDeltaWire(
      baseRevision: 7,
      revision: 8,
      beforeHash32: 0x11223344,
      afterHash32: 0xAABBCCDD,
      startIndex: 9,
      deleteCount: 2,
      inserted: const [
        PrototypeParsedBlockWire(
          stableId: 0x001FFFFFFFFF,
          sourceUtf8Length: 42,
          sourceUtf16Length: 40,
          hiddenRanges: [PrototypeLocalRangeWire(startUtf16: 2, endUtf16: 4)],
          replacements: [
            PrototypeLocalReplacementWire(
              startUtf16: 8,
              endUtf16: 10,
              text: 'é',
            ),
          ],
        ),
      ],
    );

    final encoded = delta.encode();
    expect(encoded.length, 78);
    expect(
      _hex(encoded),
      '464c443301000000070000000800000044332211ddccbbaa0900000002000000'
      '01000000ffffffff1f0000002a00000028000000010001000200000004000000'
      '080000000a00000002000000c3a9',
    );

    final decoded = PrototypeSyntaxDeltaWire.decode(encoded);
    expect(decoded.baseRevision, 7);
    expect(decoded.revision, 8);
    expect(decoded.beforeHash32, 0x11223344);
    expect(decoded.afterHash32, 0xAABBCCDD);
    expect(decoded.startIndex, 9);
    expect(decoded.deleteCount, 2);
    expect(decoded.inserted.single.stableId, 0x001FFFFFFFFF);
    expect(decoded.inserted.single.sourceUtf8Length, 42);
    expect(decoded.inserted.single.sourceUtf16Length, 40);
    expect(decoded.inserted.single.hiddenRanges.single.startUtf16, 2);
    expect(decoded.inserted.single.hiddenRanges.single.endUtf16, 4);
    expect(decoded.inserted.single.replacements.single.text, 'é');
  });

  test('decoder rejects truncated, trailing, and version-skewed payloads', () {
    final valid = PrototypeSyntaxDeltaWire(
      baseRevision: 0,
      revision: 1,
      beforeHash32: 1,
      afterHash32: 2,
      startIndex: 0,
      deleteCount: 0,
      inserted: const [],
    ).encode();

    expect(
      () => PrototypeSyntaxDeltaWire.decode(
        Uint8List.sublistView(valid, 0, valid.length - 1),
      ),
      throwsFormatException,
    );
    expect(
      () => PrototypeSyntaxDeltaWire.decode(Uint8List.fromList([...valid, 0])),
      throwsFormatException,
    );
    final wrongVersion = Uint8List.fromList(valid)..[4] = 2;
    expect(
      () => PrototypeSyntaxDeltaWire.decode(wrongVersion),
      throwsFormatException,
    );
  });
}

String _hex(Uint8List bytes) =>
    bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
