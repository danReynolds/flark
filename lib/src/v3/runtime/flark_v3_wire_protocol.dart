import 'dart:typed_data';

/// Byte order and framing shared by the native FFI and Web Worker/Wasm v3
/// transports.
///
/// Rust object layouts never cross this boundary. All multibyte values are
/// unsigned little-endian integers and every frame must be consumed exactly.
final class FlarkV3WireProtocol {
  const FlarkV3WireProtocol._();

  static const int headerBytes = 24;
  static const int abiMajor = 1;
  static const int abiMinor = 1;
  static const int maximumPayloadBytes = 256 * 1024;
  static const int allowedFlags = 0;

  static const List<int> _magic = <int>[0x46, 0x4c, 0x4b, 0x33]; // FLK3

  static Uint8List encode(FlarkV3WireFrame frame) {
    _validateUnsigned32(frame.flags, 'flags');
    _validateUnsigned32(frame.correlationId, 'correlationId');
    if (frame.flags & ~allowedFlags != 0) {
      throw ArgumentError.value(frame.flags, 'flags', 'contains unknown bits');
    }
    if (frame.payload.length > maximumPayloadBytes) {
      throw RangeError.range(
        frame.payload.length,
        0,
        maximumPayloadBytes,
        'payload.length',
      );
    }
    if (frame.kind == FlarkV3WireFrameKind.request &&
        frame.status != FlarkV3WireStatus.ok) {
      throw ArgumentError('A request frame must carry status OK.');
    }

    final output = Uint8List(headerBytes + frame.payload.length);
    output.setRange(0, _magic.length, _magic);
    final data = ByteData.sublistView(output);
    data
      ..setUint16(4, abiMajor, Endian.little)
      ..setUint16(6, abiMinor, Endian.little)
      ..setUint16(8, frame.opcode.code, Endian.little)
      ..setUint16(10, frame.status.code, Endian.little)
      ..setUint32(12, frame.flags, Endian.little)
      ..setUint32(16, frame.correlationId, Endian.little)
      ..setUint32(20, frame.payload.length, Endian.little);
    output.setRange(headerBytes, output.length, frame.payload);
    return output;
  }

  /// Decodes one already-owned bounded transport buffer.
  ///
  /// The returned payload is a view over [bytes]. The caller must therefore
  /// transfer or retain the input buffer for exactly as long as the frame is
  /// consumed; decoding never creates a second payload-sized allocation.
  static FlarkV3WireFrame decode(
    Uint8List bytes, {
    required FlarkV3WireFrameKind kind,
    int supportedMinor = abiMinor,
    int maximumPayload = maximumPayloadBytes,
    int allowedFlagMask = allowedFlags,
  }) {
    if (maximumPayload < 0 || maximumPayload > maximumPayloadBytes) {
      throw RangeError.range(
        maximumPayload,
        0,
        maximumPayloadBytes,
        'maximumPayload',
      );
    }
    if (supportedMinor < 0 || supportedMinor > 0xffff) {
      throw RangeError.range(supportedMinor, 0, 0xffff, 'supportedMinor');
    }
    if (bytes.length < headerBytes) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.truncatedHeader,
        byteOffset: bytes.length,
        expected: headerBytes,
        actual: bytes.length,
      );
    }
    for (var index = 0; index < _magic.length; index += 1) {
      if (bytes[index] != _magic[index]) {
        throw FlarkV3WireFormatException(
          FlarkV3WireFailure.invalidMagic,
          byteOffset: index,
          expected: _magic[index],
          actual: bytes[index],
        );
      }
    }

    final data = ByteData.sublistView(bytes, 0, headerBytes);
    final major = data.getUint16(4, Endian.little);
    if (major != abiMajor) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.unsupportedMajor,
        byteOffset: 4,
        expected: abiMajor,
        actual: major,
      );
    }
    final minor = data.getUint16(6, Endian.little);
    if (minor > supportedMinor) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.unsupportedMinor,
        byteOffset: 6,
        expected: supportedMinor,
        actual: minor,
      );
    }

    final opcodeCode = data.getUint16(8, Endian.little);
    final opcode = FlarkV3WireOpcode.fromCode(opcodeCode);
    if (opcode == null) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.unknownOpcode,
        byteOffset: 8,
        actual: opcodeCode,
      );
    }
    final statusCode = data.getUint16(10, Endian.little);
    final status = FlarkV3WireStatus.fromCode(statusCode);
    if (status == null) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.unknownStatus,
        byteOffset: 10,
        actual: statusCode,
      );
    }
    if (kind == FlarkV3WireFrameKind.request &&
        status != FlarkV3WireStatus.ok) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.requestStatusNotZero,
        byteOffset: 10,
        expected: FlarkV3WireStatus.ok.code,
        actual: statusCode,
      );
    }

    final flags = data.getUint32(12, Endian.little);
    if (flags & ~allowedFlagMask != 0) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.forbiddenFlags,
        byteOffset: 12,
        expected: allowedFlagMask,
        actual: flags,
      );
    }
    final correlationId = data.getUint32(16, Endian.little);
    final payloadLength = data.getUint32(20, Endian.little);
    if (payloadLength > maximumPayload) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.payloadTooLarge,
        byteOffset: 20,
        expected: maximumPayload,
        actual: payloadLength,
      );
    }
    final expectedLength = headerBytes + payloadLength;
    if (bytes.length < expectedLength) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.truncatedPayload,
        byteOffset: bytes.length,
        expected: expectedLength,
        actual: bytes.length,
      );
    }
    if (bytes.length > expectedLength) {
      throw FlarkV3WireFormatException(
        FlarkV3WireFailure.trailingBytes,
        byteOffset: expectedLength,
        expected: expectedLength,
        actual: bytes.length,
      );
    }

    return FlarkV3WireFrame.owned(
      kind: kind,
      opcode: opcode,
      status: status,
      flags: flags,
      correlationId: correlationId,
      payload: Uint8List.sublistView(bytes, headerBytes, expectedLength),
    );
  }
}

enum FlarkV3WireFrameKind { request, response }

/// Stable transport opcodes. Grammar and storage payload schemas are versioned
/// inside the corresponding frame; adding an opcode never reuses a value.
enum FlarkV3WireOpcode {
  close(0x0001),
  drain(0x0002),
  hostOpen(0x0100),
  observeSource(0x0101),
  publishBegin(0x0110),
  publishPacket(0x0111),
  publishCommit(0x0112),
  publishAbort(0x0113),
  hostPoll(0x0120),
  query(0x0130),
  acknowledgeDelivery(0x0140),
  parserOpen(0x0200),
  snapshotPage(0x0201),
  edit(0x0202),
  parserRefineInline(0x0203),
  parserPresentViewport(0x0204),
  parserPoll(0x0210),
  supersede(0x0211),
  parserAcknowledge(0x0220);

  const FlarkV3WireOpcode(this.code);

  final int code;

  static final Map<int, FlarkV3WireOpcode> _byCode = {
    for (final value in values) value.code: value,
  };

  static FlarkV3WireOpcode? fromCode(int code) => _byCode[code];
}

/// Stable status space shared by Dart, native Rust, and Wasm.
enum FlarkV3WireStatus {
  ok(0x0000),
  unsupportedMajor(0x0010),
  unsupportedMinor(0x0011),
  malformedFrame(0x0012),
  unknownOpcode(0x0013),
  forbiddenFlags(0x0014),
  invalid(0x0100),
  invalidState(0x0101),
  backpressure(0x0102),
  staleSource(0x0103),
  exactSourceMismatch(0x0104),
  sessionSnapshotRequired(0x0105),
  baseMismatch(0x0106),
  wrongOffer(0x0107),
  corruptPayload(0x0108),
  queryBoundExceeded(0x0109),
  foregroundBoundExceeded(0x010a),
  superseded(0x010b),
  closed(0x010c),
  notReady(0x010d),
  unsupportedSchema(0x010e),
  allocationFailed(0x010f),
  invalidUtf8(0x0110),
  internalFault(0x0111);

  const FlarkV3WireStatus(this.code);

  final int code;

  static final Map<int, FlarkV3WireStatus> _byCode = {
    for (final value in values) value.code: value,
  };

  static FlarkV3WireStatus? fromCode(int code) => _byCode[code];
}

final class FlarkV3WireFrame {
  FlarkV3WireFrame.copied({
    required this.kind,
    required this.opcode,
    this.status = FlarkV3WireStatus.ok,
    this.flags = 0,
    required this.correlationId,
    required List<int> payload,
  }) : payload = Uint8List.fromList(payload);

  FlarkV3WireFrame.owned({
    required this.kind,
    required this.opcode,
    this.status = FlarkV3WireStatus.ok,
    this.flags = 0,
    required this.correlationId,
    required this.payload,
  });

  final FlarkV3WireFrameKind kind;
  final FlarkV3WireOpcode opcode;
  final FlarkV3WireStatus status;
  final int flags;
  final int correlationId;
  final Uint8List payload;
}

enum FlarkV3WireFailure {
  truncatedHeader,
  invalidMagic,
  unsupportedMajor,
  unsupportedMinor,
  unknownOpcode,
  unknownStatus,
  requestStatusNotZero,
  forbiddenFlags,
  payloadTooLarge,
  truncatedPayload,
  trailingBytes,
}

/// Local structured decode failure. No arbitrary Rust error string is placed
/// on the production wire; callers can map this stable tuple to diagnostics.
final class FlarkV3WireFormatException implements FormatException {
  const FlarkV3WireFormatException(
    this.failure, {
    required this.byteOffset,
    this.expected,
    this.actual,
  });

  final FlarkV3WireFailure failure;
  final int byteOffset;
  final int? expected;
  final int? actual;

  @override
  String get message => 'Invalid Flark v3 frame: ${failure.name}';

  @override
  int get offset => byteOffset;

  @override
  Object? get source => null;

  @override
  String toString() =>
      'FlarkV3WireFormatException(${failure.name}, offset: $byteOffset, '
      'expected: $expected, actual: $actual)';
}

void _validateUnsigned32(int value, String name) {
  if (value < 0 || value > 0xffffffff) {
    throw RangeError.range(value, 0, 0xffffffff, name);
  }
}
