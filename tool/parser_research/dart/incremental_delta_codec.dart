import 'dart:convert';
import 'dart:typed_data';

const int _magic = 0x33444C46; // "FLD3" in little-endian byte order.
const int prototypeSyntaxDeltaProtocolVersion = 1;

/// Disposable binary protocol prototype for authoritative parser deltas.
///
/// The source text is intentionally absent: Dart already owns the canonical
/// source rope. Blocks carry stable identity, source metrics, and local
/// projection facts only.
final class PrototypeSyntaxDeltaWire {
  const PrototypeSyntaxDeltaWire({
    required this.baseRevision,
    required this.revision,
    required this.beforeHash32,
    required this.afterHash32,
    required this.startIndex,
    required this.deleteCount,
    required this.inserted,
  });

  final int baseRevision;
  final int revision;
  final int beforeHash32;
  final int afterHash32;
  final int startIndex;
  final int deleteCount;
  final List<PrototypeParsedBlockWire> inserted;

  Uint8List encode() {
    final writer = _Writer()
      ..u32(_magic)
      ..u16(prototypeSyntaxDeltaProtocolVersion)
      ..u16(0)
      ..u32(baseRevision)
      ..u32(revision)
      ..u32(beforeHash32)
      ..u32(afterHash32)
      ..u32(startIndex)
      ..u32(deleteCount)
      ..u32(inserted.length);
    for (final block in inserted) {
      writer
        ..u64(block.stableId)
        ..u32(block.sourceUtf8Length)
        ..u32(block.sourceUtf16Length)
        ..u16(block.hiddenRanges.length)
        ..u16(block.replacements.length);
      for (final range in block.hiddenRanges) {
        writer
          ..u32(range.startUtf16)
          ..u32(range.endUtf16);
      }
      for (final replacement in block.replacements) {
        final text = Uint8List.fromList(utf8.encode(replacement.text));
        writer
          ..u32(replacement.startUtf16)
          ..u32(replacement.endUtf16)
          ..u32(text.length)
          ..bytes(text);
      }
    }
    return writer.takeBytes();
  }

  static PrototypeSyntaxDeltaWire decode(Uint8List bytes) {
    final reader = _Reader(bytes);
    if (reader.u32() != _magic) {
      throw const FormatException('Invalid syntax-delta magic.');
    }
    final version = reader.u16();
    if (version != prototypeSyntaxDeltaProtocolVersion) {
      throw FormatException('Unsupported syntax-delta version $version.');
    }
    final flags = reader.u16();
    if (flags != 0) {
      throw FormatException('Unsupported syntax-delta flags $flags.');
    }
    final baseRevision = reader.u32();
    final revision = reader.u32();
    final beforeHash32 = reader.u32();
    final afterHash32 = reader.u32();
    final startIndex = reader.u32();
    final deleteCount = reader.u32();
    final insertCount = reader.u32();
    final inserted = <PrototypeParsedBlockWire>[];
    for (var index = 0; index < insertCount; index += 1) {
      final stableId = reader.u64();
      final sourceUtf8Length = reader.u32();
      final sourceUtf16Length = reader.u32();
      final hiddenCount = reader.u16();
      final replacementCount = reader.u16();
      final hiddenRanges = <PrototypeLocalRangeWire>[];
      for (var rangeIndex = 0; rangeIndex < hiddenCount; rangeIndex += 1) {
        hiddenRanges.add(
          PrototypeLocalRangeWire(
            startUtf16: reader.u32(),
            endUtf16: reader.u32(),
          ),
        );
      }
      final replacements = <PrototypeLocalReplacementWire>[];
      for (
        var replacementIndex = 0;
        replacementIndex < replacementCount;
        replacementIndex += 1
      ) {
        final start = reader.u32();
        final end = reader.u32();
        final textLength = reader.u32();
        replacements.add(
          PrototypeLocalReplacementWire(
            startUtf16: start,
            endUtf16: end,
            text: utf8.decode(reader.bytes(textLength)),
          ),
        );
      }
      inserted.add(
        PrototypeParsedBlockWire(
          stableId: stableId,
          sourceUtf8Length: sourceUtf8Length,
          sourceUtf16Length: sourceUtf16Length,
          hiddenRanges: hiddenRanges,
          replacements: replacements,
        ),
      );
    }
    if (!reader.isDone) {
      throw const FormatException('Trailing syntax-delta bytes.');
    }
    return PrototypeSyntaxDeltaWire(
      baseRevision: baseRevision,
      revision: revision,
      beforeHash32: beforeHash32,
      afterHash32: afterHash32,
      startIndex: startIndex,
      deleteCount: deleteCount,
      inserted: inserted,
    );
  }
}

final class PrototypeParsedBlockWire {
  const PrototypeParsedBlockWire({
    required this.stableId,
    required this.sourceUtf8Length,
    required this.sourceUtf16Length,
    required this.hiddenRanges,
    required this.replacements,
  });

  final int stableId;
  final int sourceUtf8Length;
  final int sourceUtf16Length;
  final List<PrototypeLocalRangeWire> hiddenRanges;
  final List<PrototypeLocalReplacementWire> replacements;
}

final class PrototypeLocalRangeWire {
  const PrototypeLocalRangeWire({
    required this.startUtf16,
    required this.endUtf16,
  });

  final int startUtf16;
  final int endUtf16;
}

final class PrototypeLocalReplacementWire {
  const PrototypeLocalReplacementWire({
    required this.startUtf16,
    required this.endUtf16,
    required this.text,
  });

  final int startUtf16;
  final int endUtf16;
  final String text;
}

final class _Writer {
  final BytesBuilder _output = BytesBuilder(copy: false);

  void u16(int value) {
    if (value < 0 || value > 0xFFFF) {
      throw RangeError.range(value, 0, 0xFFFF);
    }
    final data = ByteData(2)..setUint16(0, value, Endian.little);
    _output.add(data.buffer.asUint8List());
  }

  void u32(int value) {
    if (value < 0 || value > 0xFFFFFFFF) {
      throw RangeError.range(value, 0, 0xFFFFFFFF);
    }
    final data = ByteData(4)..setUint32(0, value, Endian.little);
    _output.add(data.buffer.asUint8List());
  }

  void u64(int value) {
    if (value < 0 || value > 0x1FFFFFFFFFFFFF) {
      throw RangeError.range(value, 0, 0x1FFFFFFFFFFFFF);
    }
    u32(value & 0xFFFFFFFF);
    u32(value ~/ 0x100000000);
  }

  void bytes(Uint8List value) => _output.add(value);

  Uint8List takeBytes() => _output.takeBytes();
}

final class _Reader {
  _Reader(this._bytes);

  final Uint8List _bytes;
  var _offset = 0;

  bool get isDone => _offset == _bytes.length;

  int u16() {
    _require(2);
    final value = ByteData.sublistView(
      _bytes,
      _offset,
      _offset + 2,
    ).getUint16(0, Endian.little);
    _offset += 2;
    return value;
  }

  int u32() {
    _require(4);
    final value = ByteData.sublistView(
      _bytes,
      _offset,
      _offset + 4,
    ).getUint32(0, Endian.little);
    _offset += 4;
    return value;
  }

  int u64() {
    final low = u32();
    final high = u32();
    final value = high * 0x100000000 + low;
    if (value > 0x1FFFFFFFFFFFFF) {
      throw const FormatException(
        'Stable ID exceeds exact Dart integer range.',
      );
    }
    return value;
  }

  Uint8List bytes(int length) {
    _require(length);
    final value = Uint8List.sublistView(_bytes, _offset, _offset + length);
    _offset += length;
    return value;
  }

  void _require(int length) {
    if (length < 0 || _offset + length > _bytes.length) {
      throw const FormatException('Truncated syntax delta.');
    }
  }
}
