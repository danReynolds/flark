import 'dart:async';
import 'dart:typed_data';

import 'package:flark/flark.dart';
import 'package:fleury/fleury_core.dart';
import 'package:fleury_widgets/fleury_widgets_web.dart' as widgets;
import 'package:http/http.dart' as http;
import 'image_decode.dart';

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
  PreviewCancellation? _decode;

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
    _decode?.cancel();
    final decode = _decode = PreviewCancellation();
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
        cancellation: decode,
      ).timeout(const Duration(seconds: 15));
      if (mounted && epoch == _epoch) setState(() => _source = source);
    } catch (_) {
      if (mounted && epoch == _epoch) {
        setState(() => _error = 'Could not load image');
      }
    } finally {
      decode.cancel();
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
    _decode?.cancel();
    _client?.close();
    super.dispose();
  }
}

/// Internal loader, kept separate from widget lifetime for transport tests.
Future<widgets.ImageSource> loadPreviewImage(
  Uri uri,
  http.Client client, {
  PreviewCancellation? cancellation,
}) async {
  final token = cancellation ?? PreviewCancellation();
  const maxBytes = 4 * 1024 * 1024;
  token.check();
  final response = await client.send(http.Request('GET', uri));
  if (response.statusCode != 200 || (response.contentLength ?? 0) > maxBytes) {
    throw const FormatException('Image response exceeds preview limits');
  }
  final bytes = BytesBuilder(copy: false);
  await for (final chunk in response.stream) {
    token.check();
    if (bytes.length + chunk.length > maxBytes) {
      throw const FormatException('Image response exceeds preview limits');
    }
    bytes.add(chunk);
  }
  token.check();
  final prepared = await previewDecoder.decode(bytes.takeBytes(), token);
  return widgets.ImageSource.decoded(prepared.image, encodedPng: prepared.png);
}
