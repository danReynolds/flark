import '../flark_v3_byte_endpoint.dart';

/// Owned default-platform endpoint used by the public Dart runtime facade.
///
/// [done] completes only after the platform worker has released its native or
/// Wasm document slot. Keeping this ownership receipt beside the byte endpoint
/// lets the public facade make `close()` mean actual runtime reclamation rather
/// than merely sending a close command.
final class FlarkV3PlatformEndpointHandle {
  const FlarkV3PlatformEndpointHandle({
    required this.endpoint,
    required this.done,
  });

  final FlarkV3ByteEndpoint endpoint;
  final Future<void> done;
}
