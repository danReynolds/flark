import 'dart:typed_data';

import 'package:flark_fleury/src/image_previews.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:image/image.dart' as pixels;
import 'package:test/test.dart';

void main() {
  final uri = Uri.parse('https://images.example/demo.png');
  final png = pixels.encodePng(pixels.Image(width: 16, height: 8));
  test('decodes an ordinary image and preserves its aspect ratio', () async {
    final client = MockClient((_) async => http.Response.bytes(png, 200));
    addTearDown(client.close);
    final image = (await loadPreviewImage(uri, client)).decode();
    expect((image.width, image.height), (16, 8));
  });
  test('rejects errors and invalid image content', () async {
    for (final response in [
      http.Response('not found', 404),
      http.Response('not an image', 200),
    ]) {
      final client = MockClient((_) async => response);
      addTearDown(client.close);
      await expectLater(loadPreviewImage(uri, client), throwsFormatException);
    }
  });
  test('enforces the byte limit without trusting content length', () async {
    final client = MockClient.streaming(
      (_, _) async => http.StreamedResponse(
        Stream.fromIterable([
          Uint8List(3 * 1024 * 1024),
          Uint8List(2 * 1024 * 1024),
        ]),
        200,
      ),
    );
    addTearDown(client.close);
    await expectLater(loadPreviewImage(uri, client), throwsFormatException);
  });
  test('rejects excessive pixel dimensions before decoding', () async {
    final data = pixels.encodePng(pixels.Image(width: 2049, height: 2049));
    final client = MockClient((_) async => http.Response.bytes(data, 200));
    addTearDown(client.close);
    await expectLater(loadPreviewImage(uri, client), throwsFormatException);
  });
}
