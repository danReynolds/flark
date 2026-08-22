import 'dart:typed_data';

import 'package:flark/src/v3/runtime/flark_v3_wire_protocol.dart';
import 'package:test/test.dart';

void main() {
  group('FlarkV3WireProtocol', () {
    test('encodes the canonical little-endian host-open request', () {
      final encoded = FlarkV3WireProtocol.encode(
        FlarkV3WireFrame.copied(
          kind: FlarkV3WireFrameKind.request,
          opcode: FlarkV3WireOpcode.hostOpen,
          correlationId: 0x01020304,
          payload: const [0xaa, 0xbb, 0xcc],
        ),
      );

      expect(
        encoded,
        orderedEquals(const [
          0x46,
          0x4c,
          0x4b,
          0x33,
          0x01,
          0x00,
          0x01,
          0x00,
          0x00,
          0x01,
          0x00,
          0x00,
          0x00,
          0x00,
          0x00,
          0x00,
          0x04,
          0x03,
          0x02,
          0x01,
          0x03,
          0x00,
          0x00,
          0x00,
          0xaa,
          0xbb,
          0xcc,
        ]),
      );
    });

    test('round trips an owned response without a payload copy', () {
      final bytes = FlarkV3WireProtocol.encode(
        FlarkV3WireFrame.copied(
          kind: FlarkV3WireFrameKind.response,
          opcode: FlarkV3WireOpcode.parserPoll,
          status: FlarkV3WireStatus.notReady,
          correlationId: 7,
          payload: const [1, 2, 3],
        ),
      );
      final decoded = FlarkV3WireProtocol.decode(
        bytes,
        kind: FlarkV3WireFrameKind.response,
      );

      expect(decoded.opcode, FlarkV3WireOpcode.parserPoll);
      expect(decoded.status, FlarkV3WireStatus.notReady);
      expect(decoded.correlationId, 7);
      expect(decoded.payload, orderedEquals(const [1, 2, 3]));
      bytes[FlarkV3WireProtocol.headerBytes] = 9;
      expect(decoded.payload.first, 9, reason: 'decoder returns an owned view');
    });

    test('rejects every truncated header length', () {
      for (
        var length = 0;
        length < FlarkV3WireProtocol.headerBytes;
        length += 1
      ) {
        _expectFailure(Uint8List(length), FlarkV3WireFailure.truncatedHeader);
      }
    });

    test('rejects invalid magic and unsupported ABI versions', () {
      final valid = _request();
      _expectMutation(valid, 0, 0, FlarkV3WireFailure.invalidMagic);
      _expectMutation(valid, 4, 2, FlarkV3WireFailure.unsupportedMajor);
      _expectMutation(valid, 6, 2, FlarkV3WireFailure.unsupportedMinor);
    });

    test('rejects unknown opcode and response status', () {
      final request = _request();
      request[8] = 0xff;
      request[9] = 0x7f;
      _expectFailure(request, FlarkV3WireFailure.unknownOpcode);

      final response = _response();
      response[10] = 0xff;
      response[11] = 0x7f;
      _expectFailure(
        response,
        FlarkV3WireFailure.unknownStatus,
        kind: FlarkV3WireFrameKind.response,
      );
    });

    test('rejects a non-zero request status and forbidden flags', () {
      final status = _request();
      status[10] = FlarkV3WireStatus.notReady.code & 0xff;
      status[11] = FlarkV3WireStatus.notReady.code >> 8;
      _expectFailure(status, FlarkV3WireFailure.requestStatusNotZero);

      final flags = _request();
      flags[12] = 1;
      _expectFailure(flags, FlarkV3WireFailure.forbiddenFlags);
    });

    test('distinguishes oversized, truncated, and trailing payloads', () {
      final oversized = _request();
      ByteData.sublistView(oversized).setUint32(
        20,
        FlarkV3WireProtocol.maximumPayloadBytes + 1,
        Endian.little,
      );
      _expectFailure(oversized, FlarkV3WireFailure.payloadTooLarge);

      final truncated = _request(payload: const [1, 2, 3]);
      _expectFailure(
        Uint8List.sublistView(truncated, 0, truncated.length - 1),
        FlarkV3WireFailure.truncatedPayload,
      );

      final valid = _request(payload: const [1]);
      final trailing = Uint8List(valid.length + 1)
        ..setRange(0, valid.length, valid);
      _expectFailure(trailing, FlarkV3WireFailure.trailingBytes);
    });

    test('enforces a caller-selected smaller payload ceiling', () {
      final encoded = _request(payload: const [1, 2, 3]);
      expect(
        () => FlarkV3WireProtocol.decode(
          encoded,
          kind: FlarkV3WireFrameKind.request,
          maximumPayload: 2,
        ),
        throwsA(_failure(FlarkV3WireFailure.payloadTooLarge)),
      );
    });

    test('encoder rejects invalid request authority before allocation', () {
      expect(
        () => FlarkV3WireProtocol.encode(
          FlarkV3WireFrame.copied(
            kind: FlarkV3WireFrameKind.request,
            opcode: FlarkV3WireOpcode.close,
            status: FlarkV3WireStatus.closed,
            correlationId: 1,
            payload: const [],
          ),
        ),
        throwsArgumentError,
      );
      expect(
        () => FlarkV3WireProtocol.encode(
          FlarkV3WireFrame.copied(
            kind: FlarkV3WireFrameKind.request,
            opcode: FlarkV3WireOpcode.close,
            correlationId: 0x100000000,
            payload: const [],
          ),
        ),
        throwsRangeError,
      );
    });
  });
}

Uint8List _request({List<int> payload = const []}) =>
    FlarkV3WireProtocol.encode(
      FlarkV3WireFrame.copied(
        kind: FlarkV3WireFrameKind.request,
        opcode: FlarkV3WireOpcode.hostOpen,
        correlationId: 1,
        payload: payload,
      ),
    );

Uint8List _response() => FlarkV3WireProtocol.encode(
  FlarkV3WireFrame.copied(
    kind: FlarkV3WireFrameKind.response,
    opcode: FlarkV3WireOpcode.hostOpen,
    correlationId: 1,
    payload: const [],
  ),
);

void _expectMutation(
  Uint8List original,
  int offset,
  int value,
  FlarkV3WireFailure failure,
) {
  final bytes = Uint8List.fromList(original)..[offset] = value;
  _expectFailure(bytes, failure);
}

void _expectFailure(
  Uint8List bytes,
  FlarkV3WireFailure failure, {
  FlarkV3WireFrameKind kind = FlarkV3WireFrameKind.request,
}) {
  expect(
    () => FlarkV3WireProtocol.decode(bytes, kind: kind),
    throwsA(_failure(failure)),
  );
}

Matcher _failure(FlarkV3WireFailure failure) =>
    isA<FlarkV3WireFormatException>().having(
      (error) => error.failure,
      'failure',
      failure,
    );
