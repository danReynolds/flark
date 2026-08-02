/// External assets used by the production Web Worker + WebAssembly runtime.
///
/// Both URLs are explicit so applications can serve immutable, fingerprinted
/// files under their own Content Security Policy. The worker is a standalone
/// external script; Flark never synthesizes a `blob:` worker or evaluates
/// downloaded source.
final class FlarkV3WebRuntimeAssets {
  const FlarkV3WebRuntimeAssets({
    required this.workerUri,
    required this.wasmUri,
  });

  /// URL of the external Flark parser Worker script.
  final Uri workerUri;

  /// URL fetched by that Worker and by the independent main-context host.
  final Uri wasmUri;

  /// Conventional URLs exposed by a Dart package-test/package-server setup.
  ///
  /// Production applications should normally pass their own fingerprinted
  /// URLs because Flutter Web and other bundlers choose different public asset
  /// prefixes.
  factory FlarkV3WebRuntimeAssets.packageDefaults({Uri? baseUri}) {
    final base = baseUri ?? Uri.base;
    return FlarkV3WebRuntimeAssets(
      workerUri: base.resolve(
        'packages/flark/assets/worker/flark_v3_parser_worker.js',
      ),
      wasmUri: base.resolve(
        'packages/flark/assets/wasm/flark_comrak_bridge.wasm',
      ),
    );
  }
}
