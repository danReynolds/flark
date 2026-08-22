import 'dart:typed_data';

import 'package:flark/src/v3/host/host.dart';

FlarkV3HostPublicationPacket testPublicationPacket({
  required FlarkV3OfferId offerId,
  required int firstFrameOrdinal,
  required int firstRecordOrdinal,
  required int recordCount,
  required FlarkV3ProtocolDigest128 digest,
  required Uint8List frameBytes,
}) {
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  final rawBytes = Uint8List(bodyOffset + frameBytes.length);
  final data = ByteData.sublistView(rawBytes);

  void writeId(int offset, FlarkV3ProtocolId128 id) {
    data
      ..setUint32(offset, id.word0, Endian.little)
      ..setUint32(offset + 4, id.word1, Endian.little)
      ..setUint32(offset + 8, id.word2, Endian.little)
      ..setUint32(offset + 12, id.word3, Endian.little);
  }

  rawBytes.setRange(0, 4, const <int>[0x46, 0x50, 0x4b, 0x33]);
  data
    ..setUint16(4, FlarkV3HostPublicationPacket.wireVersion, Endian.little)
    ..setUint16(6, FlarkV3HostPublicationPacket.wireFlags, Endian.little);
  writeId(8, offerId);
  data
    ..setUint32(24, firstFrameOrdinal, Endian.little)
    ..setUint32(28, firstRecordOrdinal, Endian.little)
    ..setUint32(32, 1, Endian.little)
    ..setUint32(36, recordCount, Endian.little)
    ..setUint32(40, frameBytes.length, Endian.little)
    ..setUint32(
      FlarkV3HostPublicationPacket.wireHeaderBytes,
      frameBytes.length,
      Endian.little,
    )
    ..setUint32(
      FlarkV3HostPublicationPacket.wireHeaderBytes + 4,
      recordCount,
      Endian.little,
    );
  writeId(FlarkV3HostPublicationPacket.wireHeaderBytes + 8, digest);
  rawBytes.setRange(bodyOffset, rawBytes.length, frameBytes);
  return FlarkV3HostPublicationPacket.fromOwnedBytes(rawBytes);
}

Uint8List testSingleFrameBody(FlarkV3HostPublicationPacket packet) {
  if (packet.frameCount != 1) {
    throw ArgumentError.value(packet.frameCount, 'packet.frameCount');
  }
  final bodyOffset =
      FlarkV3HostPublicationPacket.wireHeaderBytes +
      FlarkV3HostPublicationPacket.wireFrameDirectoryEntryBytes;
  return Uint8List.sublistView(packet.rawBytes, bodyOffset);
}
