import 'dart:typed_data';
import 'dart:math' as math;
import 'package:image/image.dart' as pixels;

class PreviewCancelled implements Exception {
  const PreviewCancelled();
}

/// One mounted load owns this token; cancellation drops queued work and kills
/// a running native decoder. Browser work retains its queue slot until settled.
class PreviewCancellation {
  bool cancelled = false;
  final _listeners = <void Function()>{};
  void check() {
    if (cancelled) throw const PreviewCancelled();
  }

  void Function() listen(void Function() callback) {
    if (cancelled) {
      callback();
      return () {};
    }
    _listeners.add(callback);
    return () => _listeners.remove(callback);
  }

  void cancel() {
    if (cancelled) return;
    cancelled = true;
    for (final callback in _listeners.toList()) {
      callback();
    }
    _listeners.clear();
  }
}

class PreviewPixels {
  PreviewPixels(this.image, this.png);
  final pixels.Image image;
  final Uint8List png;
}

(pixels.Decoder, int, int) checkedPreviewDecoder(Uint8List bytes) {
  final decoder = pixels.findDecoderForData(bytes);
  final info = decoder?.startDecode(bytes);
  if (info == null ||
      info.width <= 0 ||
      info.height <= 0 ||
      info.width * info.height > 4 * 1024 * 1024) {
    throw const FormatException('Image dimensions exceed preview limits');
  }
  return (decoder!, info.width, info.height);
}

(int, int) previewSize(int width, int height) {
  final scale = math.min(1.0, math.min(960 / width, 640 / height));
  return (
    math.max(1, (width * scale).floor()),
    math.max(1, (height * scale).floor()),
  );
}
