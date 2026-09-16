import 'dart:async';
import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart' as widgets;
import 'package:http/http.dart' as http;
import 'package:image/image.dart' as pixels;

/// Replace the preview widget to resolve application assets, authenticated
/// images, or a different loading/error presentation. Constraints are the fixed
/// preview slot; the host continues to own selection and resource actions.
typedef FlarkFleuryImagePreviewBuilder =
    Widget Function(BuildContext context, InlineResource resource, Uri? uri);

/// Bounded HTTP image preview. Only visible previews are mounted by the host;
/// leaving the viewport closes the request and drops the decoded pixels.
/// Browser loading follows normal CORS rules. Only the first frame is decoded.
class FlarkImagePreview extends StatefulWidget {
  const FlarkImagePreview({super.key, required this.uri, required this.label});
  final Uri? uri;
  final String label;

  @override
  State<FlarkImagePreview> createState() => _ImagePreviewState();
}

class _ImagePreviewState extends State<FlarkImagePreview> {
  http.Client? _client;
  widgets.ImageSource? _source;
  String? _error;
  int _epoch = 0;

  @override
  void initState() {
    super.initState();
    _load();
  }

  @override
  void didUpdateWidget(FlarkImagePreview oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.uri != widget.uri) _load();
  }

  Future<void> _load() async {
    final epoch = ++_epoch;
    _client?.close();
    _source = null;
    _error = null;
    final uri = widget.uri;
    if (uri == null || !(uri.scheme == 'http' || uri.scheme == 'https')) {
      _error = 'Image preview unavailable';
      return;
    }
    final client = _client = http.Client();
    try {
      final source = await loadPreviewImage(
        uri,
        client,
      ).timeout(const Duration(seconds: 15));
      if (mounted && epoch == _epoch) setState(() => _source = source);
    } catch (_) {
      if (mounted && epoch == _epoch) {
        setState(() => _error = 'Could not load image');
      }
    } finally {
      client.close();
    }
  }

  @override
  Widget build(BuildContext context) {
    final source = _source;
    return source != null
        ? widgets.Image(
            source: source,
            semanticLabel: widget.label,
            backgroundColor: Theme.of(context).colorScheme.background,
          )
        : Center(
            child: Text(
              _error ?? 'Loading image…',
              maxLines: 2,
              style: Theme.of(context).mutedStyle,
            ),
          );
  }

  @override
  void dispose() {
    _epoch++;
    _client?.close();
    super.dispose();
  }
}

/// Internal loader, kept separate from widget lifetime for transport tests.
Future<widgets.ImageSource> loadPreviewImage(
  Uri uri,
  http.Client client,
) async {
  const maxBytes = 4 * 1024 * 1024, maxPixels = 4 * 1024 * 1024;
  final response = await client.send(http.Request('GET', uri));
  if (response.statusCode != 200 || (response.contentLength ?? 0) > maxBytes) {
    throw const FormatException('Image response exceeds preview limits');
  }
  final bytes = BytesBuilder(copy: false);
  await for (final chunk in response.stream) {
    if (bytes.length + chunk.length > maxBytes) {
      throw const FormatException('Image response exceeds preview limits');
    }
    bytes.add(chunk);
  }
  final encoded = bytes.takeBytes();
  final decoder = pixels.findDecoderForData(encoded);
  final info = decoder?.startDecode(encoded);
  if (info == null ||
      info.width <= 0 ||
      info.height <= 0 ||
      info.width * info.height > maxPixels) {
    throw const FormatException('Image dimensions exceed preview limits');
  }
  final decoded = decoder!.decodeFrame(0);
  if (decoded == null) throw const FormatException('Unsupported image');
  // Keep the mounted preview's retained pixels bounded as well as the input.
  final image = decoded.width > 960 || decoded.height > 640
      ? pixels.copyResize(
          decoded,
          width: 960,
          height: 640,
          maintainAspect: true,
          interpolation: pixels.Interpolation.average,
        )
      : decoded;
  return widgets.ImageSource.decoded(image);
}
