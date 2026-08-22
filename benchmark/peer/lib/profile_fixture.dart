import 'dart:convert';

import 'package:crypto/crypto.dart';

const competitorProtocolId = 'm0-mac-competitor-profile-v1';
const ordinaryProseGeneratorId = 'flark-v4-deterministic-markdown-v1';
const ordinaryProseShapeId = 'ordinary-prose';

const ordinaryProseCycle =
    'Ordinary prose opens with a clear sentence and a small **bold** run.\n'
    'It continues with _emphasis_, `code`, and a direct '
    '[link](https://example.invalid/).\n\n';

const pastePayloadBytes = 32768;

/// Implements the M0 `repeat_ascii_exact` recipe without normalization.
String generateOrdinaryProseExact(int targetBytes) {
  if (targetBytes < 0) {
    throw ArgumentError.value(
      targetBytes,
      'targetBytes',
      'must be nonnegative',
    );
  }
  assert(utf8.encode(ordinaryProseCycle).length == ordinaryProseCycle.length);

  final completeCycles = targetBytes ~/ ordinaryProseCycle.length;
  final remainder = targetBytes % ordinaryProseCycle.length;
  final buffer = StringBuffer();
  for (var index = 0; index < completeCycles; index += 1) {
    buffer.write(ordinaryProseCycle);
  }
  buffer.write(ordinaryProseCycle.substring(0, remainder));
  final result = buffer.toString();
  if (result.length != targetBytes ||
      utf8.encode(result).length != targetBytes) {
    throw StateError(
      'repeat_ascii_exact generated ${utf8.encode(result).length} bytes, '
      'expected $targetBytes',
    );
  }
  return result;
}

String sha256Text(String value) =>
    sha256.convert(utf8.encode(value)).toString();

Map<String, Object?> compareSource({
  required String expected,
  required String actual,
}) {
  final sharedLength = expected.length < actual.length
      ? expected.length
      : actual.length;
  var firstDifference = sharedLength;
  for (var index = 0; index < sharedLength; index += 1) {
    if (expected.codeUnitAt(index) != actual.codeUnitAt(index)) {
      firstDifference = index;
      break;
    }
  }
  final exact = expected == actual;
  final contextStart = (firstDifference - 24).clamp(0, sharedLength).toInt();
  final expectedContextEnd = (firstDifference + 48)
      .clamp(0, expected.length)
      .toInt();
  final actualContextEnd = (firstDifference + 48)
      .clamp(0, actual.length)
      .toInt();
  final terminalNewlineOnly = actual == '$expected\n';

  return <String, Object?>{
    'exact': exact,
    'expectedUtf8Bytes': utf8.encode(expected).length,
    'actualUtf8Bytes': utf8.encode(actual).length,
    'expectedSha256': sha256Text(expected),
    'actualSha256': sha256Text(actual),
    'firstDifferenceUtf16Offset': exact ? null : firstDifference,
    'lengthDeltaUtf16': actual.length - expected.length,
    'classification': exact
        ? 'exact'
        : terminalNewlineOnly
        ? 'peer-appended-terminal-newline'
        : 'source-difference',
    'expectedContext': exact
        ? null
        : expected.substring(contextStart, expectedContextEnd),
    'actualContext': exact
        ? null
        : actual.substring(contextStart, actualContextEnd),
  };
}
