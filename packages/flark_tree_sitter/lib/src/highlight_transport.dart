import 'dart:typed_data';

/// Internal worker transport. The queue allows only one outstanding call.
abstract interface class HighlightTransport {
  Future<Uint8List> analyze(String source, int language);
  void dispose();
}
