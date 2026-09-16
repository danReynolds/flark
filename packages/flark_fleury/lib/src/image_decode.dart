import 'dart:async';
import 'dart:typed_data';
import 'image_decode_native.dart'
    if (dart.library.js_interop) 'image_decode_web.dart';
import 'image_decode_types.dart';
export 'image_decode_types.dart' show PreviewCancellation, PreviewCancelled;

/// Shared backpressure across mounted editors. Cancelled browser operations
/// keep their active slot until the native promise settles, bounding real work.
final previewDecoder = PreviewDecodeQueue();

class PreviewDecodeQueue {
  static const maxActive = 2, maxQueued = 8;
  final _queue = <_Job>[];
  int _active = 0;
  int get active => _active;
  int get queued => _queue.length;

  Future<PreviewPixels> decode(Uint8List bytes, PreviewCancellation token) {
    if (token.cancelled) return Future.error(const PreviewCancelled());
    if (bytes.length > 4 * 1024 * 1024 || _queue.length >= maxQueued) {
      return Future.error(const FormatException('Image preview queue is full'));
    }
    final job = _Job(bytes, token);
    _queue.add(job);
    job.remove = token.listen(() {
      if (_queue.remove(job) && !job.done.isCompleted) {
        job.remove?.call();
        job.done.completeError(const PreviewCancelled());
      }
    });
    _drain();
    return job.done.future;
  }

  void _drain() {
    while (_active < maxActive && _queue.isNotEmpty) {
      final job = _queue.removeAt(0);
      _active++;
      unawaited(_run(job));
    }
  }

  Future<void> _run(_Job job) async {
    try {
      final result = await decodePreviewPlatform(job.bytes, job.token);
      job.token.check();
      job.done.complete(result);
    } catch (e, st) {
      job.done.completeError(e, st);
    } finally {
      job.remove?.call();
      _active--;
      _drain();
    }
  }
}

class _Job {
  _Job(this.bytes, this.token);
  final Uint8List bytes;
  final PreviewCancellation token;
  final done = Completer<PreviewPixels>();
  void Function()? remove;
}
