import 'package:flutter/painting.dart';
import 'resource_dialog.dart' show flarkResourceUri;

/// Supply asset/file/authenticated image providers for document resources.
/// Returning null displays an unavailable preview. The default allows HTTP(S).
typedef FlarkImageProvider = ImageProvider<Object>? Function(Uri uri);

class PreviewImage {
  ImageInfo? info;
  bool failed = false, disposed = false;
  ImageStream? stream;
  ImageStreamListener? listener;
  void dispose() {
    disposed = true;
    if (listener != null) stream?.removeListener(listener!);
    info?.dispose();
    info = null;
  }
}

/// Surface-owned, bounded image streams. Geometry never depends on loading.
class SurfaceImageCache {
  SurfaceImageCache(this.changed);
  final VoidCallback changed;
  final _entries = <Uri, PreviewImage>{};
  Uri? base;
  FlarkImageProvider? provider;

  void configure(Uri? nextBase, FlarkImageProvider? nextProvider) {
    if (base == nextBase && provider == nextProvider) return;
    clear();
    base = nextBase;
    provider = nextProvider;
  }

  PreviewImage? get(String destination) =>
      _entries[flarkResourceUri(destination, base)];

  void visible(Iterable<String> destinations) {
    final needed = destinations
        .map((s) => flarkResourceUri(s, base))
        .whereType<Uri>()
        .toSet()
        .take(16)
        .toSet();
    // Drop off-screen streams before starting new requests. Retain only the
    // visible set, at most sixteen decodes of at most 960 by 640 pixels.
    for (final key in _entries.keys.toList()) {
      if (!needed.contains(key)) _entries.remove(key)!.dispose();
    }
    for (final uri in needed) {
      if (_entries.containsKey(uri)) continue;
      final entry = PreviewImage();
      _entries[uri] = entry;
      try {
        final image = provider != null
            ? provider!(uri)
            : const {'http', 'https'}.contains(uri.scheme)
            ? NetworkImage(uri.toString())
            : null;
        if (image == null) {
          entry.failed = true;
          continue;
        }
        final stream = ResizeImage(
          image,
          width: 960,
          height: 640,
          policy: ResizeImagePolicy.fit,
        ).resolve(ImageConfiguration.empty);
        entry.stream = stream;
        final listener = ImageStreamListener(
          (info, synchronous) {
            if (entry.disposed) {
              info.dispose();
              return;
            }
            entry.info?.dispose();
            entry.info = info;
            if (!synchronous) changed();
          },
          onError: (Object error, StackTrace? stack) {
            if (!entry.disposed) {
              entry.failed = true;
              changed();
            }
          },
        );
        entry.listener = listener;
        stream.addListener(listener);
      } catch (_) {
        entry.failed = true;
      }
    }
  }

  void clear() {
    for (final entry in _entries.values) {
      entry.dispose();
    }
    _entries.clear();
  }
}
