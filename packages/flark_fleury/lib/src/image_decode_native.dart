import 'dart:async';
import 'dart:isolate';
import 'dart:typed_data';
import 'package:image/image.dart' as pixels;
import 'image_decode_types.dart';

void _prepare((SendPort, TransferableTypedData) request) {
  try {
    final bytes = request.$2.materialize().asUint8List();
    final (decoder, w, h) = checkedPreviewDecoder(bytes);
    final decoded = decoder.decodeFrame(0);
    if (decoded == null) throw const FormatException('Unsupported image');
    final (width, height) = previewSize(w, h);
    final image = width == w && height == h
        ? decoded
        : pixels.copyResize(
            decoded,
            width: width,
            height: height,
            interpolation: pixels.Interpolation.average,
          );
    // Both expensive transforms finish before any pixels reach the UI isolate.
    final png = Uint8List.fromList(pixels.encodePng(image));
    Isolate.exit(request.$1, [image, png]);
  } catch (error) {
    Isolate.exit(request.$1, error.toString());
  }
}

Future<PreviewPixels> decodePreviewPlatform(
  Uint8List bytes,
  PreviewCancellation token,
) async {
  token.check();
  final port = ReceivePort(), done = Completer<PreviewPixels>();
  Isolate? isolate;
  final remove = token.listen(() {
    isolate?.kill(priority: Isolate.immediate);
    if (!done.isCompleted) done.completeError(const PreviewCancelled());
  });
  final subscription = port.listen((message) {
    if (done.isCompleted) return;
    if (message is List && message.length == 2 && message[0] is pixels.Image) {
      done.complete(
        PreviewPixels(message[0] as pixels.Image, message[1] as Uint8List),
      );
    } else {
      done.completeError(FormatException('Image decode failed: $message'));
    }
  });
  // Install the error handler before a cancelled spawn can complete its future.
  final result = done.future;
  unawaited(result.then<void>((_) {}, onError: (Object _) {}));
  try {
    isolate = await Isolate.spawn(
      _prepare,
      (port.sendPort, TransferableTypedData.fromList([bytes])),
      onError: port.sendPort,
      onExit: port.sendPort,
      errorsAreFatal: true,
      debugName: 'Flark image preview',
    );
    if (token.cancelled) isolate.kill(priority: Isolate.immediate);
    return await result;
  } finally {
    remove();
    isolate?.kill(priority: Isolate.immediate);
    await subscription.cancel();
    port.close();
  }
}
