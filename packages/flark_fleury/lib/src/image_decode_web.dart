import 'dart:js_interop';
import 'dart:typed_data';
import 'package:image/image.dart' as pixels;
import 'package:web/web.dart' as web;
import 'image_decode_types.dart';

Future<PreviewPixels> decodePreviewPlatform(
  Uint8List bytes,
  PreviewCancellation token,
) async {
  token.check();
  // Inspect the bounded encoded header before asking the browser to allocate
  // pixels. Dart does no full-frame decoding, resizing or PNG encoding here.
  checkedPreviewDecoder(bytes);
  final blob = web.Blob([bytes.toJS].toJS);
  // The browser applies orientation metadata before reporting bitmap size.
  // Resizing from the stored header's aspect ratio stretches rotated photos.
  final bitmap = await web.window.createImageBitmap(blob).toDart;
  web.ImageBitmap? resized;
  try {
    token.check();
    final (width, height) = previewSize(bitmap.width, bitmap.height);
    if (width != bitmap.width || height != bitmap.height) {
      resized = await web.window
          .createImageBitmap(
            bitmap,
            web.ImageBitmapOptions(
              resizeWidth: width,
              resizeHeight: height,
              resizeQuality: 'high',
            ),
          )
          .toDart;
      token.check();
    }
    final canvas = web.OffscreenCanvas(width, height);
    final context =
        canvas.getContext('2d')! as web.OffscreenCanvasRenderingContext2D;
    context.drawImage(
      resized ?? bitmap,
      0,
      0,
      width.toDouble(),
      height.toDouble(),
    );
    final data = context.getImageData(0, 0, width, height).data.toDart;
    final png = await canvas
        .convertToBlob(web.ImageEncodeOptions(type: 'image/png'))
        .toDart;
    token.check();
    final buffer = await png.arrayBuffer().toDart;
    token.check();
    return PreviewPixels(
      pixels.Image.fromBytes(
        width: width,
        height: height,
        bytes: data.buffer,
        bytesOffset: data.offsetInBytes,
        numChannels: 4,
      ),
      buffer.toDart.asUint8List(),
    );
  } finally {
    resized?.close();
    bitmap.close();
  }
}
