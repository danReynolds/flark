import 'models.dart';

/// Narrow source port required by bounded viewport navigation.
///
/// The production implementation is [FlarkCoreDocument]. Tests can supply a
/// deterministic in-memory source without opening a native document.
abstract interface class FlarkViewportSource {
  int get sourceByteLength;

  Future<FlarkViewport> queryViewport({
    int startByte = 0,
    int? endByte,
    int maxRows = 256,
  });

  Future<FlarkViewport> queryViewportNext(
    FlarkViewport previous, {
    int maxRows = 256,
  });

  Future<void> releaseViewportContinuation(FlarkViewport viewport);

  Future<String> readSourceRange(int startByte, int endByte);
}
