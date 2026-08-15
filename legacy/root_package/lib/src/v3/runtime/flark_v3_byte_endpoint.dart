import 'dart:typed_data';

/// Receives one owned frame from a platform parser endpoint.
///
/// Native isolate and Web Worker adapters transfer the backing buffer before
/// invoking this callback. The receiver may retain the frame until it has
/// synchronously decoded the credited event.
typedef FlarkV3ByteFrameCallback = void Function(Uint8List frame);

/// Reports an asynchronous platform-endpoint failure.
///
/// This includes an isolate/Worker crash and a native/Wasm endpoint status
/// that could not be represented by a credited FLK3 `Failed` event. The typed
/// wire transport remains the sole owner of the fail-closed policy.
typedef FlarkV3ByteEndpointFailureCallback =
    void Function(Object error, StackTrace stackTrace);

/// Exact endpoint identity needed to construct a replacement worker slot.
///
/// Recovery construction sits below FLK3 command dispatch: native uses these
/// six words to call `flark_v3_endpoint_recover`, while Web uses the same
/// identity to allocate a replacement Wasm endpoint. Keeping this as values
/// avoids teaching either platform adapter how to decode parser-open frames.
final class FlarkV3ByteEndpointBinding {
  FlarkV3ByteEndpointBinding({
    required List<int> documentSessionWords,
    required this.sourceSessionIdentity,
    required this.workerGeneration,
  }) : documentSessionWords = List<int>.unmodifiable(documentSessionWords) {
    if (documentSessionWords.length != 4) {
      throw ArgumentError.value(
        documentSessionWords,
        'documentSessionWords',
        'must contain exactly four u32 words',
      );
    }
    for (final word in documentSessionWords) {
      _requireU32(word, 'documentSessionWords');
    }
    _requirePositiveU32(sourceSessionIdentity, 'sourceSessionIdentity');
    _requirePositiveU32(workerGeneration, 'workerGeneration');
  }

  final List<int> documentSessionWords;
  final int sourceSessionIdentity;
  final int workerGeneration;

  @override
  bool operator ==(Object other) =>
      other is FlarkV3ByteEndpointBinding &&
      _sameWords(other.documentSessionWords, documentSessionWords) &&
      other.sourceSessionIdentity == sourceSessionIdentity &&
      other.workerGeneration == workerGeneration;

  @override
  int get hashCode => Object.hash(
    documentSessionWords[0],
    documentSessionWords[1],
    documentSessionWords[2],
    documentSessionWords[3],
    sourceSessionIdentity,
    workerGeneration,
  );
}

/// Smallest platform-specific seam below Flark's typed parser transport.
///
/// Implementations only move bounded, versioned frames. They do not interpret
/// Markdown, source revisions, parser events, or publication state. Native
/// implementations own a long-lived isolate and registry handle; web
/// implementations own a long-lived Worker and Wasm endpoint slot.
abstract interface class FlarkV3ByteEndpoint {
  /// Installs the endpoint's only inbound-frame and failure callbacks.
  void bind({
    required FlarkV3ByteFrameCallback onFrame,
    required FlarkV3ByteEndpointFailureCallback onFailure,
  });

  /// Orders an abnormal endpoint replacement before the next command frame.
  ///
  /// Implementations create the recovery endpoint first, atomically swap it
  /// into the serialized adapter, and only then emergency-revoke the old
  /// endpoint. A subsequent [send] must observe that ordering.
  void recover(FlarkV3ByteEndpointBinding previousBinding);

  /// Transfers ownership of one bounded command frame to the endpoint.
  void send(Uint8List frame);

  /// Transfers one terminal schema-2 host-poll result through its dedicated
  /// native/Wasm entrypoint.
  ///
  /// The typed wire owner selects this path. Platform adapters do not inspect
  /// command bytes to rediscover publication protocol routing.
  void sendHostPoll(Uint8List frame);

  /// Transfers one terminal hot-inline sidecar host-poll result through its
  /// dedicated native/Wasm entrypoint.
  ///
  /// The sibling sidecar protocol deliberately cannot enter the structural
  /// publication host-poll lane. Platform adapters preserve that separation
  /// without decoding the command payload.
  void sendInlineSidecarHostPoll(Uint8List frame);

  /// Transfers one terminal VPB1 host-poll result through its dedicated
  /// native/Wasm entrypoint.
  void sendViewportPresentationHostPoll(Uint8List frame);

  /// Transfers an exact begin-close frame through the strict close entrypoint.
  ///
  /// The typed wire owner selects this path; platform adapters still do not
  /// inspect the frame to decide whether endpoint shutdown was requested.
  void sendClose(Uint8List frame);

  /// Releases platform resources. Repeated calls must be harmless.
  void close();
}

const int _maximumU32 = 0xffffffff;

void _requireU32(int value, String name) {
  if (value < 0 || value > _maximumU32) {
    throw RangeError.range(value, 0, _maximumU32, name);
  }
}

void _requirePositiveU32(int value, String name) {
  if (value <= 0 || value > _maximumU32) {
    throw RangeError.range(value, 1, _maximumU32, name);
  }
}

bool _sameWords(List<int> left, List<int> right) {
  for (var index = 0; index < 4; index += 1) {
    if (left[index] != right[index]) return false;
  }
  return true;
}
