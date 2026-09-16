import 'package:flark_fleury/src/image_decode.dart';
import 'package:image/image.dart' as pixels;
import 'package:test/test.dart';

void main() {
  for (final (width, height, outWidth, outHeight) in [
    (40, 20, 20, 40),
    (800, 600, 480, 640),
    (1600, 900, 360, 640),
  ]) {
    test(
      'EXIF rotation precedes thumbnail sizing for $width x $height',
      () async {
        final input = pixels.Image(width: width, height: height);
        pixels.fill(input, color: pixels.ColorRgb8(220, 20, 30));
        pixels.fillRect(
          input,
          x1: 0,
          y1: height ~/ 2,
          x2: width - 1,
          y2: height - 1,
          color: pixels.ColorRgb8(20, 30, 220),
        );
        input.exif.imageIfd.orientation = 6;
        final result = await PreviewDecodeQueue().decode(
          pixels.encodeJpg(input),
          PreviewCancellation(),
        );
        expect(
          (result.image.width, result.image.height),
          (outWidth, outHeight),
        );
        final png = pixels.decodePng(result.png)!;
        expect((png.width, png.height), (outWidth, outHeight));
        // Clockwise rotation places the blue bottom half on the left.
        final left = result.image.getPixel(outWidth ~/ 4, outHeight ~/ 2);
        final right = result.image.getPixel(outWidth * 3 ~/ 4, outHeight ~/ 2);
        expect(left.b, greaterThan(left.r));
        expect(right.r, greaterThan(right.b));
      },
    );
  }
}
